//! `ra2tool`: command-line tools for working with a game install.

// Surveys and the dispatch table are long but flat.
#![allow(clippy::too_many_lines)]

mod audio;
mod csf;
mod font;
mod image;
mod ini;
mod map;
mod mix;
mod pal;
mod shp;
mod tmp;
mod vfs;
mod vxl;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ra2_vfs::install::{Candidate, Game, Inspection, Origin, SearchContext, find_in};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Find the game install and check it has what each game needs.
    Doctor {
        /// Check this folder instead of searching.
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Work with MIX archives. Archive paths are a file on disk or in the
    /// game install, then any nested archives: `ra2md.mix/localmd.mix`.
    #[command(subcommand)]
    Mix(MixCommand),
    /// The game's file system, mounted the way the game mounts it at startup.
    #[command(subcommand)]
    Vfs(VfsCommand),
    /// Read INI files from the game's file system, as the game reads them.
    #[command(subcommand)]
    Ini(IniCommand),
    /// Read the game's string tables.
    #[command(subcommand)]
    Csf(CsfCommand),
    /// Palettes and house colours.
    #[command(subcommand)]
    Pal(PalCommand),
    /// SHP sprites.
    #[command(subcommand)]
    Shp(ShpCommand),
    /// Voxel models and animations.
    #[command(subcommand)]
    Vxl(VxlCommand),
    /// Theaters and their terrain tiles.
    #[command(subcommand)]
    Tmp(TmpCommand),
    /// Scenario files.
    #[command(subcommand)]
    Map(MapCommand),
    /// Sounds, music and the sound bank.
    #[command(subcommand)]
    Audio(AudioCommand),
    /// The game's bitmap font.
    #[command(subcommand)]
    Font(FontCommand),
}

#[derive(Subcommand)]
enum FontCommand {
    /// Describe game.fnt and check every glyph it maps.
    Check {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Decode every PCX image in the mounted game.
    Pcx {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Render a line of text.
    Render {
        text: String,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum AudioCommand {
    /// Decode every WAV, AUD and bank sound in the mounted game.
    Check {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Export a sound (a file name, or a bank sound's name) as a PCM WAV.
    Export {
        name: String,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum MapCommand {
    /// Decode every map in the game and check its tiles against its theater.
    Check {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Summarise a map.
    Info {
        file: String,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Draw a map to an image.
    Render {
        file: String,
        #[arg(short, long)]
        out: PathBuf,
        /// Shrink by this factor (nearest pixel).
        #[arg(long, default_value_t = 1)]
        scale: u32,
        /// Draw only around this cell: x,y.
        #[arg(long)]
        around: Option<String>,
        /// The size of the --around view in pixels: `WxH`.
        #[arg(long, default_value = "800x600")]
        size: String,
        /// Shroud what the map's player can't see at the start.
        #[arg(long)]
        shroud: bool,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Open a map in a window. Drag or arrows pan, scroll or +/- zoom, 0
    /// resets; O, L and S toggle objects, lighting and shroud.
    View {
        file: String,
        /// Start with the player's shroud on.
        #[arg(long)]
        shroud: bool,
        /// Don't open a window: draw the opening view through the GPU to
        /// this PNG.
        #[arg(long)]
        snapshot: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Write a map's embedded preview image.
    Preview {
        file: String,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum TmpCommand {
    /// Mount each theater and parse every tile its INI lists.
    Check {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Draw a tile template (e.g. clear01.tem) in its theater's palette.
    Png {
        theater: String,
        file: String,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum VxlCommand {
    /// Describe a model (`name`, without .vxl) and its animation.
    Info {
        name: String,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Draw a model's sections from the side and from above.
    Png {
        name: String,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value = "unittem.pal")]
        palette: String,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Parse every VXL and HVA in the mounted game and pair them by name.
    Check {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum ShpCommand {
    /// Draw frames in a palette.
    Png {
        file: String,
        #[arg(short, long)]
        out: PathBuf,
        #[arg(long, default_value = "unittem.pal")]
        palette: String,
        /// A [Colors] name, or H,S,V, for the remap slots.
        #[arg(long)]
        house: Option<String>,
        /// First frame to draw.
        #[arg(long, default_value_t = 0)]
        first: usize,
        /// How many frames to draw (default: all).
        #[arg(long)]
        count: Option<usize>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Try every file in the mounted game as an SHP and decode every frame.
    Check {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum PalCommand {
    /// Draw a palette's 256 colours, optionally with a house colour applied.
    Png {
        file: String,
        #[arg(short, long)]
        out: PathBuf,
        /// A [Colors] name from the rules, or H,S,V.
        #[arg(long)]
        house: Option<String>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Print the 16-colour remap ramp for house colours (all of [Colors] if none given).
    Ramp {
        colours: Vec<String>,
        #[arg(short, long)]
        out: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum CsfCommand {
    /// Print labels' text, or with --grep the labels whose text matches.
    Get {
        labels: Vec<String>,
        #[arg(long)]
        grep: Option<String>,
        #[arg(long, default_value = "ra2md.csf")]
        file: String,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Summarise a table.
    Stats {
        #[arg(long, default_value = "ra2md.csf")]
        file: String,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum IniCommand {
    /// Print a value, or a whole section when no key is given.
    Get {
        file: String,
        section: String,
        key: Option<String>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Summarise how files parse: sections, entries, repeats, long lines.
    Stats {
        files: Vec<String>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum GameArg {
    Yr,
    Ra2,
}

impl From<GameArg> for Game {
    fn from(g: GameArg) -> Game {
        match g {
            GameArg::Yr => Game::YurisRevenge,
            GameArg::Ra2 => Game::RedAlert2,
        }
    }
}

#[derive(Subcommand)]
enum VfsCommand {
    /// Mount the startup archives in order and show what each step did.
    Mounts {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Show where files resolve: a loose file, or which mounted archive.
    Which {
        names: Vec<String>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum MixCommand {
    /// List an archive's files.
    Ls {
        archive: String,
        /// A file of extra names to recognise, one per line.
        #[arg(long)]
        names: Option<PathBuf>,
    },
    /// Extract files by name, to a folder or to standard output.
    X {
        archive: String,
        files: Vec<String>,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Build an unencrypted archive from files on disk (for mods and
    /// experiments; never commit one holding game data).
    Create { out: PathBuf, files: Vec<PathBuf> },
    /// Open every archive in the install, and those nested in them, and
    /// check their indexes and checksums.
    Check {
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Doctor { game_dir } => doctor(game_dir),
        Command::Font(cmd) => exit(match cmd {
            FontCommand::Check { game, game_dir } => font::check(game.into(), game_dir),
            FontCommand::Pcx { game, game_dir } => font::check_pcx(game.into(), game_dir),
            FontCommand::Render {
                text,
                out,
                game,
                game_dir,
            } => font::render(game.into(), game_dir, &text, &out),
        }),
        Command::Audio(cmd) => exit(match cmd {
            AudioCommand::Check { game, game_dir } => audio::check(game.into(), game_dir),
            AudioCommand::Export {
                name,
                out,
                game,
                game_dir,
            } => audio::export(game.into(), game_dir, &name, &out),
        }),
        Command::Map(cmd) => exit(match cmd {
            MapCommand::Check { game, game_dir } => map::check(game.into(), game_dir.as_deref()),
            MapCommand::Render {
                file,
                out,
                scale,
                around,
                size,
                shroud,
                game,
                game_dir,
            } => {
                let around = around.and_then(|a| {
                    let (x, y) = a.split_once(',')?;
                    let (w, h) = size.split_once('x')?;
                    Some((
                        x.trim().parse().ok()?,
                        y.trim().parse().ok()?,
                        w.parse().ok()?,
                        h.parse().ok()?,
                    ))
                });
                map::render(
                    game.into(),
                    game_dir.as_deref(),
                    &file,
                    scale,
                    around,
                    shroud,
                    &out,
                )
            }
            MapCommand::View {
                file,
                shroud,
                snapshot,
                game,
                game_dir,
            } => map::view(
                game.into(),
                game_dir.as_deref(),
                &file,
                shroud,
                snapshot.as_deref(),
            ),
            MapCommand::Info {
                file,
                game,
                game_dir,
            } => map::info(game.into(), game_dir.as_deref(), &file),
            MapCommand::Preview {
                file,
                out,
                game,
                game_dir,
            } => map::preview(game.into(), game_dir.as_deref(), &file, &out),
        }),
        Command::Tmp(cmd) => exit(match cmd {
            TmpCommand::Check { game, game_dir } => tmp::check(game.into(), game_dir.as_deref()),
            TmpCommand::Png {
                theater,
                file,
                out,
                game,
                game_dir,
            } => tmp::png(game.into(), game_dir, &theater, &file, &out),
        }),
        Command::Vxl(cmd) => exit(match cmd {
            VxlCommand::Info {
                name,
                game,
                game_dir,
            } => vxl::info(game.into(), game_dir, &name),
            VxlCommand::Check { game, game_dir } => vxl::check(game.into(), game_dir),
            VxlCommand::Png {
                name,
                out,
                palette,
                game,
                game_dir,
            } => vxl::png(game.into(), game_dir, &name, &palette, &out),
        }),
        Command::Shp(cmd) => exit(match cmd {
            ShpCommand::Png {
                file,
                out,
                palette,
                house,
                first,
                count,
                game,
                game_dir,
            } => shp::png(
                game.into(),
                game_dir,
                &file,
                &palette,
                house.as_deref(),
                Some((first, count.unwrap_or(usize::MAX))),
                &out,
            ),
            ShpCommand::Check { game, game_dir } => shp::check(game.into(), game_dir),
        }),
        Command::Pal(cmd) => exit(match cmd {
            PalCommand::Png {
                file,
                out,
                house,
                game,
                game_dir,
            } => pal::png(game.into(), game_dir, &file, house.as_deref(), &out),
            PalCommand::Ramp {
                colours,
                out,
                game,
                game_dir,
            } => pal::ramps(game.into(), game_dir, &colours, out.as_deref()),
        }),
        Command::Csf(cmd) => exit(match cmd {
            CsfCommand::Get {
                labels,
                grep,
                file,
                game,
                game_dir,
            } => csf::get(game.into(), game_dir, &file, &labels, grep.as_deref()),
            CsfCommand::Stats {
                file,
                game,
                game_dir,
            } => csf::stats(game.into(), game_dir, &file),
        }),
        Command::Ini(cmd) => exit(match cmd {
            IniCommand::Get {
                file,
                section,
                key,
                game,
                game_dir,
            } => ini::get(game.into(), game_dir, &file, &section, key.as_deref()),
            IniCommand::Stats {
                files,
                game,
                game_dir,
            } => ini::stats(game.into(), game_dir, &files),
        }),
        Command::Vfs(cmd) => exit(match cmd {
            VfsCommand::Mounts { game, game_dir } => vfs::mounts(game.into(), game_dir),
            VfsCommand::Which {
                names,
                game,
                game_dir,
            } => vfs::which(game.into(), game_dir, &names),
        }),
        Command::Mix(cmd) => exit(match cmd {
            MixCommand::Ls { archive, names } => mix::ls(&archive, names.as_deref()).map(|()| true),
            MixCommand::X {
                archive,
                files,
                out,
            } => mix::extract(&archive, &files, out.as_deref()).map(|()| true),
            MixCommand::Check { game_dir } => mix::check(game_dir),
            MixCommand::Create { out, files } => mix::create(&out, &files).map(|()| true),
        }),
    }
}

fn exit(result: std::io::Result<bool>) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn doctor(game_dir: Option<PathBuf>) -> ExitCode {
    let ctx = SearchContext::from_env();
    let candidates = match game_dir {
        Some(path) => vec![Candidate {
            path,
            origin: Origin::Given,
        }],
        None => ctx.candidates(),
    };
    if let Some(cfg) = ctx.config_file() {
        println!("config file: {}", cfg.display());
    }
    let install = match find_in(candidates, Game::YurisRevenge) {
        Ok(i) => i,
        Err(e) => {
            eprint!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let i: &Inspection = &install.inspection;
    println!("found:       {} ({})", i.root.display(), install.origin);
    println!("edition:     {}", i.edition);
    println!(
        "gamemd.exe:  {}",
        i.gamemd_version.unwrap_or("absent or unknown version")
    );
    println!(
        "game.exe:    {}",
        i.game_version.unwrap_or("absent or unknown version")
    );
    for game in [Game::YurisRevenge, Game::RedAlert2] {
        let missing = i.missing(game);
        if missing.is_empty() {
            println!("{:<16} ok", format!("{}:", game.name()));
        } else {
            println!(
                "{:<16} missing {}",
                format!("{}:", game.name()),
                missing.join(", ")
            );
        }
    }
    ExitCode::SUCCESS
}
