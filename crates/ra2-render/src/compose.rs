//! Composing a whole map to an image: terrain, objects, lighting and shroud.
//! Shared by `ra2tool map render` and the viewer.

use std::collections::BTreeSet;
use std::io;

use ra2_formats::ini::{Ini, atoi};
use ra2_formats::map::Map;
use ra2_sim::coords::Cell;
use ra2_vfs::mount::Vfs;
use ra2_vfs::theater::Theater;

use crate::canvas::Canvas;
use crate::lighting::Lighting;
use crate::scene::{self, Art, SceneStats};
use crate::shroud;
use crate::terrain::{self, TerrainStats, TileLibrary};

/// What to draw: independent switches, so plain booleans.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layers {
    pub objects: bool,
    pub lighting: bool,
    /// Shroud everything the map's player house can't see at the start.
    pub shroud: bool,
    /// The map's own buildings, vehicles and infantry (off when a running
    /// game draws them).
    pub technos: bool,
}

impl Default for Layers {
    fn default() -> Layers {
        Layers {
            objects: true,
            lighting: true,
            shroud: false,
            technos: true,
        }
    }
}

/// A map loaded with everything needed to draw it.
#[derive(Debug)]
pub struct MapView {
    pub name: String,
    pub map: Map,
    pub ini: Ini,
    pub theater: Theater,
    lib: TileLibrary,
    art: Art,
    /// The soft-edge shroud, when `gamemd.exe` and `shroud.shp` are found.
    soft_shroud: Option<(shroud::FrameTable, shroud::Shapes)>,
}

/// What a composition drew.
#[derive(Debug, Default, Clone)]
pub struct Stats {
    pub terrain: TerrainStats,
    pub scene: SceneStats,
    pub revealed: usize,
}

impl MapView {
    /// Load a map by file name, mounting its theater onto `vfs`.
    pub fn load(vfs: &mut Vfs, name: &str, rules: &str, art: &str) -> io::Result<MapView> {
        let data = vfs
            .read(name)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{name}: not found")))?;
        MapView::from_data(vfs, name, &data, rules, art)
    }

    /// Load a map from its bytes (a file from outside the game, say).
    pub fn from_data(
        vfs: &mut Vfs,
        name: &str,
        data: &[u8],
        rules: &str,
        art: &str,
    ) -> io::Result<MapView> {
        let read = |vfs: &Vfs, n: &str| {
            vfs.read(n)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{n}: not found")))
        };
        let ini = Ini::parse(data);
        let map = Map::from_ini(&ini)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let theater = Theater::from_name(&map.theater).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown theater {}", map.theater),
            )
        })?;
        vfs.mount_theater(theater);
        let lib = TileLibrary::load(vfs, theater)?;
        let art = Art::load(
            vfs,
            theater,
            Ini::parse(&read(vfs, rules)?),
            Ini::parse(&read(vfs, art)?),
            &ini,
        );
        let soft_shroud = vfs
            .read("gamemd.exe")
            .ok()
            .flatten()
            .and_then(|exe| shroud::FrameTable::from_gamemd(&exe))
            .zip(
                vfs.read("shroud.shp")
                    .ok()
                    .flatten()
                    .and_then(|d| shroud::Shapes::parse(&d)),
            );
        Ok(MapView {
            soft_shroud,
            name: name.to_string(),
            map,
            ini,
            theater,
            lib,
            art,
        })
    }

    /// Draw the map over a world-pixel rectangle, or the whole map.
    pub fn compose(
        &mut self,
        vfs: &Vfs,
        layers: Layers,
        rect: Option<(i32, i32, i32, i32)>,
    ) -> (Canvas, Stats) {
        let (x0, y0, x1, y1) = rect.unwrap_or_else(|| terrain::bounds(&self.map));
        let mut canvas = Canvas::new(x0, y0, x1, y1, [0, 0, 0, 255]);
        let light = if layers.lighting {
            Lighting::from_map(&self.ini).with_lamps(&self.ini, &self.art.rules, &self.art.art)
        } else {
            Lighting::default()
        };
        let seen = layers.shroud.then(|| self.revealed());
        let hidden = |x: i16, y: i16| seen.as_ref().is_some_and(|s| !s.contains(&(x, y)));
        let (terrain, anims) =
            terrain::draw(&mut canvas, vfs, &mut self.lib, &self.map, &light, &hidden);
        let mut stats = Stats {
            terrain,
            ..Stats::default()
        };
        if layers.objects {
            let hidden_object = |x: i32, y: i32| {
                hidden(
                    i16::try_from(x).unwrap_or(i16::MAX),
                    i16::try_from(y).unwrap_or(i16::MAX),
                )
            };
            let mut pieces = scene::collect_with(
                vfs,
                &mut self.art,
                &self.map,
                &self.ini,
                &mut stats.scene,
                &hidden_object,
                layers.technos,
            );
            pieces.extend(scene::tile_anims(vfs, &mut self.art, &anims));
            scene::draw(&mut canvas, &self.art, &light, pieces);
        }
        if let Some(seen) = &seen {
            stats.revealed = seen.len();
            match &self.soft_shroud {
                Some((table, shapes)) => {
                    shroud::draw(&mut canvas, &self.map, seen, table, shapes);
                }
                None => hard_shroud(&mut canvas, &self.map, seen),
            }
        }
        (canvas, stats)
    }

    /// A running game's frame: the live objects drawn over `backdrop` (the
    /// map composed without its own technos), lit like the map.
    pub fn draw_live(
        &mut self,
        vfs: &Vfs,
        backdrop: &Canvas,
        objects: &[scene::Live],
        lighting: bool,
    ) -> Canvas {
        let mut canvas = backdrop.clone();
        let light = if lighting {
            Lighting::from_map(&self.ini).with_lamps(&self.ini, &self.art.rules, &self.art.art)
        } else {
            Lighting::default()
        };
        let pieces = scene::live(vfs, &mut self.art, objects);
        scene::draw(&mut canvas, &self.art, &light, pieces);
        canvas
    }

    /// The cells the map's player house sees at the start: within each of
    /// its objects' `Sight=` (cells) of the object's cell. The player house
    /// is `[Basic] Player=`.
    fn revealed(&self) -> BTreeSet<(i16, i16)> {
        let mut out = BTreeSet::new();
        let Some(player) = self.ini.get("Basic", "Player") else {
            return out;
        };
        for section in ["Units", "Infantry", "Structures", "Aircraft"] {
            let Some(s) = self.ini.section(section) else {
                continue;
            };
            for (_, v) in s.entries() {
                let p: Vec<&str> = v.split(',').collect();
                if p.len() < 5 || !p[0].eq_ignore_ascii_case(player) {
                    continue;
                }
                let sight = self
                    .art
                    .rules
                    .get(p[1], "Sight")
                    .map_or(4, atoi)
                    .clamp(1, 20);
                let (cx, cy) = (atoi(p[3]), atoi(p[4]));
                for dy in -sight..=sight {
                    for dx in -sight..=sight {
                        if dx * dx + dy * dy <= sight * sight {
                            out.insert((
                                i16::try_from(cx + dx).unwrap_or(i16::MAX),
                                i16::try_from(cy + dy).unwrap_or(i16::MAX),
                            ));
                        }
                    }
                }
            }
        }
        out
    }
}

/// Without the game's shroud shapes: black out unseen cells and darken
/// seen cells next to unseen ones.
fn hard_shroud(canvas: &mut Canvas, map: &Map, seen: &BTreeSet<(i16, i16)>) {
    let diamond = |canvas: &mut Canvas, x: i16, y: i16, keep: u16| {
        let top = Cell::new(x, y).screen(0);
        for row in 0..30 {
            let half = if row < 15 {
                2 * (row + 1)
            } else {
                2 * (30 - row)
            };
            for dx in -half..half {
                if keep == 0 {
                    canvas.put(top.x + dx, top.y + row, [0, 0, 0]);
                } else {
                    canvas.shade(top.x + dx, top.y + row, keep);
                }
            }
        }
    };
    for t in &map.tiles {
        // Shroud is drawn at ground level: raised terrain shows through
        // its own shrouded cell's diamond, as in the game's view from above.
        if seen.contains(&(t.x, t.y)) {
            let edge = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                .iter()
                .any(|(dx, dy)| !seen.contains(&(t.x + dx, t.y + dy)));
            if edge {
                diamond(canvas, t.x, t.y, 140);
            }
        } else {
            diamond(canvas, t.x, t.y, 0);
        }
    }
}
