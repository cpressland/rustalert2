//! `rustalert2`: plays a parity scenario live in a window, the simulation
//! stepped at game speed and drawn between steps. The full game (menus,
//! orders, a player) arrives with M5.

use std::collections::BTreeMap;
use std::io::{self, BufWriter};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use ra2_game::{FixedStep, mix};
use ra2_parity::scenario::Scenario;
use ra2_render::canvas::Canvas;
use ra2_render::compose::{Layers, MapView};
use ra2_render::scene::{Live, LiveKind};
use ra2_render::viewer::{self, Composer, LiveFrame};
use ra2_rules::{Kind, Rules};
use ra2_sim::tick::{GameSpeed, Sim};
use ra2_vfs::mount::Vfs;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// A parity scenario (`parity/scenarios/*.toml`) to play.
    scenario: PathBuf,
    /// Game speed, 0 (fastest) to 6 (slowest).
    #[arg(long, default_value_t = 4)]
    speed: u8,
    #[arg(long, default_value_t = 1)]
    seed: u32,
    /// The player's install (default: found the usual way).
    #[arg(long)]
    game_dir: Option<PathBuf>,
    /// Instead of a window: run this many frames, then write one picture.
    #[arg(long)]
    snapshot: Option<PathBuf>,
    #[arg(long, default_value_t = 300)]
    frames: u32,
}

/// A scenario being played: the sim, and what's needed to draw it.
struct Game {
    vfs: Vfs,
    view: MapView,
    rules: Rules,
    sim: Sim,
    step: FixedStep,
    /// The map without its own units, drawn once per layer change.
    backdrop: Option<(Layers, Canvas)>,
    /// Each object's position before the last step, for blending.
    before: BTreeMap<u32, (i32, i32, i32)>,
}

impl Game {
    fn tick(&mut self) {
        self.before = self
            .sim
            .world()
            .objects
            .iter()
            .map(|o| (o.id, (o.coord.x, o.coord.y, o.coord.z)))
            .collect();
        self.sim.step(&[]);
    }

    /// The objects as the renderer wants them, `blend` of the way from
    /// their last positions to their current ones.
    fn live(&self, blend: f32) -> Vec<Live> {
        let world = self.sim.world();
        world
            .objects
            .iter()
            .map(|o| {
                let (x0, y0, z0) = self
                    .before
                    .get(&o.id)
                    .copied()
                    .unwrap_or((o.coord.x, o.coord.y, o.coord.z));
                Live {
                    kind: match o.kind {
                        Kind::Building => LiveKind::Building,
                        Kind::Vehicle => LiveKind::Vehicle,
                        Kind::Infantry => LiveKind::Infantry,
                        Kind::Aircraft => LiveKind::Aircraft,
                    },
                    type_id: self.rules.technos[o.techno].id.clone(),
                    owner: world
                        .houses
                        .get(usize::try_from(o.owner).unwrap_or(usize::MAX))
                        .map(|h| h.country.clone())
                        .unwrap_or_default(),
                    x: mix(x0, o.coord.x, blend),
                    y: mix(y0, o.coord.y, blend),
                    z: mix(z0, o.coord.z, blend),
                    // The body's facing, 16 bits to 8.
                    facing: u8::try_from(o.dir >> 8).unwrap_or(0),
                }
            })
            .collect()
    }

    /// The map without its own units, recomposed when the layers change.
    /// Returns it when it changed.
    fn backdrop(&mut self, layers: Layers) -> Option<Canvas> {
        let stale = self.backdrop.as_ref().is_none_or(|(l, _)| *l != layers);
        if !stale {
            return None;
        }
        let (canvas, _) = self.view.compose(
            &self.vfs,
            Layers {
                technos: false,
                ..layers
            },
            None,
        );
        self.backdrop = Some((layers, canvas.clone()));
        Some(canvas)
    }

    fn sprites(&mut self, layers: Layers, blend: f32) -> ra2_render::sprites::Sprites {
        let live = self.live(blend);
        self.view.live_sprites(&self.vfs, &live, layers.lighting)
    }
}

impl Composer for Game {
    /// The backdrop alone: the units are drawn over it on the GPU.
    fn compose(&mut self, layers: Layers) -> Canvas {
        self.backdrop(layers);
        self.backdrop.as_ref().expect("composed").1.clone()
    }

    fn live_frame(&mut self, layers: Layers, elapsed: Duration) -> Option<LiveFrame> {
        for _ in 0..self.step.advance(elapsed) {
            self.tick();
        }
        Some(LiveFrame {
            backdrop: self.backdrop(layers),
            sprites: {
                let blend = self.step.blend();
                self.sprites(layers, blend)
            },
        })
    }

    fn is_live(&self) -> bool {
        true
    }
}

fn start(cli: &Cli) -> io::Result<Game> {
    let scenario = Scenario::parse(&std::fs::read_to_string(&cli.scenario)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let (mut vfs, rules, map) = ra2_parity::engine::prepare(&scenario, cli.game_dir.as_deref())?;
    let world = ra2_parity::engine::setup(&scenario, &rules, &map);
    let map_text = match &scenario.generate {
        Some(spec) => ra2_parity::mapgen::write(spec).into_bytes(),
        None => vfs.read(&scenario.map)?.unwrap_or_default(),
    };
    let view = MapView::from_data(
        &mut vfs,
        &scenario.name,
        &map_text,
        "rulesmd.ini",
        "artmd.ini",
    )?;
    Ok(Game {
        vfs,
        view,
        sim: Sim::start(cli.seed, rules.clone(), world),
        rules,
        step: FixedStep::new(GameSpeed::new(cli.speed).frames_per_second()),
        backdrop: None,
        before: BTreeMap::new(),
    })
}

fn snapshot(game: &mut Game, frames: u32, out: &PathBuf) -> io::Result<()> {
    for _ in 0..frames {
        game.tick();
    }
    let layers = Layers::default();
    let backdrop = game.compose(layers);
    // Objects where the last step left them.
    let sprites = game.sprites(layers, 1.0);
    let (w, h) = (1280, 800);
    let px = viewer::snapshot_frame(
        &backdrop,
        Some(&sprites),
        w,
        h,
        None,
        viewer::Scaling::default(),
    )
    .map_err(io::Error::other)?;
    let mut enc = png::Encoder::new(BufWriter::new(std::fs::File::create(out)?), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&px))
        .map_err(io::Error::other)?;
    println!("frame {}: written to {}", game.sim.frame(), out.display());
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = start(&cli).and_then(|mut game| match &cli.snapshot {
        Some(out) => snapshot(&mut game, cli.frames, out),
        None => viewer::run(
            &format!("rustalert2: {}", cli.scenario.display()),
            Layers::default(),
            game,
        )
        .map_err(io::Error::other),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
