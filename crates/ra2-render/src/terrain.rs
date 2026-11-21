//! Terrain: each map cell's tile from its theater, raised by its level.
//! See `docs/spec/terrain.md`.

use std::collections::BTreeMap;
use std::io;

use ra2_formats::ini::Ini;
use ra2_formats::map::Map;
use ra2_formats::pal::{Palette, Rgb};
use ra2_formats::tmp::{TileSet, Tmp, diamond_rows, tile_sets};
use ra2_sim::coords::Cell;
use ra2_vfs::mount::Vfs;
use ra2_vfs::theater::Theater;

use crate::canvas::Canvas;

/// A theater's tiles, loaded on demand.
#[derive(Debug)]
pub struct TileLibrary {
    pub theater: Theater,
    pub sets: Vec<TileSet>,
    /// The tile palette (`ISO{ext}.PAL`), 8-bit.
    pub palette: [Rgb; 256],
    templates: BTreeMap<u32, Option<Tmp>>,
}

/// How a cell's tile number was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    /// The tile as numbered.
    Direct(u32),
    /// A Marble Madness tile drawn from its `NonMarbleMadness` set.
    MarbleMadness(u32),
    /// No usable tile: drawn as clear ground (tile 0).
    Clear,
}

impl TileLibrary {
    pub fn load(vfs: &Vfs, theater: Theater) -> io::Result<TileLibrary> {
        let ini = vfs.read(&theater.ini())?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{} not found", theater.ini()),
            )
        })?;
        let pal_name = &theater.palettes()[2];
        let pal = vfs
            .read(pal_name)?
            .and_then(|d| Palette::parse(&d))
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, format!("{pal_name} not found"))
            })?;
        Ok(TileLibrary {
            theater,
            sets: tile_sets(&Ini::parse(&ini)),
            palette: pal.to_rgb8(),
            templates: BTreeMap::new(),
        })
    }

    /// The set and index within it of a tile number.
    fn locate(&self, tile: u32) -> Option<(usize, u32)> {
        let mut k = tile;
        for (i, s) in self.sets.iter().enumerate() {
            if k < s.tiles {
                return Some((i, k));
            }
            k -= s.tiles;
        }
        None
    }

    /// The first tile number of set `index`.
    fn first_of(&self, index: usize) -> u32 {
        self.sets[..index].iter().map(|s| s.tiles).sum()
    }

    fn file(&self, set: usize, k: u32) -> String {
        format!(
            "{}{:02}.{}",
            self.sets[set].file_name,
            k + 1,
            self.theater.ext()
        )
    }

    fn template(&mut self, vfs: &Vfs, tile: u32) -> Option<&Tmp> {
        if !self.templates.contains_key(&tile) {
            let loaded = self
                .locate(tile)
                .and_then(|(set, k)| vfs.read(&self.file(set, k)).ok().flatten())
                .and_then(|d| Tmp::parse(&d).ok());
            self.templates.insert(tile, loaded);
        }
        self.templates.get(&tile).and_then(Option::as_ref)
    }

    /// Resolve a map cell's tile number to a template that exists.
    /// `[hypothesis]` A Marble Madness tile with no file is drawn from the
    /// set its `NonMarbleMadness` names, at the same position in the set.
    pub fn resolve(&mut self, vfs: &Vfs, tile: i32) -> Resolved {
        let Ok(tile) = u32::try_from(tile) else {
            return Resolved::Clear;
        };
        if tile == 0xFFFF {
            return Resolved::Clear;
        }
        if self.template(vfs, tile).is_some() {
            return Resolved::Direct(tile);
        }
        if let Some((set, k)) = self.locate(tile)
            && let Some(target) = self.sets[set].non_marble_madness
        {
            let target = usize::try_from(target).unwrap_or(usize::MAX);
            if target < self.sets.len() && k < self.sets[target].tiles {
                let alt = self.first_of(target) + k;
                if self.template(vfs, alt).is_some() {
                    return Resolved::MarbleMadness(alt);
                }
            }
        }
        Resolved::Clear
    }
}

/// What a terrain render did, for reports.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TerrainStats {
    pub cells: usize,
    pub marble_madness: usize,
    pub cleared: usize,
    pub missing_subtile: usize,
}

/// One drawn piece, for ordering.
struct Piece {
    order: (i32, i32, u8),
    x: i32,
    y: i32,
    width: usize,
    pixels: Vec<u8>,
    light: crate::lighting::Light,
}

/// Draw a map's terrain onto a canvas: base diamonds in row order, then
/// extra art (cliff faces and the like) in row order over them. Cells
/// `hidden` says are under shroud aren't drawn, as in the game.
pub fn draw(
    canvas: &mut Canvas,
    vfs: &Vfs,
    lib: &mut TileLibrary,
    map: &Map,
    light: &crate::lighting::Lighting,
    hidden: &dyn Fn(i16, i16) -> bool,
) -> TerrainStats {
    let mut stats = TerrainStats::default();
    let mut pieces = Vec::new();
    for t in &map.tiles {
        stats.cells += 1;
        if hidden(t.x, t.y) {
            continue;
        }
        let tile = match lib.resolve(vfs, t.tile) {
            Resolved::Direct(n) => n,
            Resolved::MarbleMadness(n) => {
                stats.marble_madness += 1;
                n
            }
            Resolved::Clear => {
                if t.tile >= 0 && t.tile != 0xFFFF {
                    stats.cleared += 1;
                }
                0
            }
        };
        let Some(tmp) = lib.template(vfs, tile) else {
            continue;
        };
        let Some(Some(cell)) = tmp
            .cells
            .get(usize::from(t.subtile))
            .or_else(|| tmp.cells.first())
        else {
            stats.missing_subtile += 1;
            continue;
        };
        let level = i32::from(t.level);
        let top = Cell::new(t.x, t.y).screen(level);
        let row = i32::from(t.x) + i32::from(t.y);
        let cell_light = light.cell(i32::from(t.x), i32::from(t.y), level);
        // The diamond, unpacked into its 60x30 box.
        let (w, h) = (tmp.tile_width as usize, tmp.tile_height as usize);
        let mut boxed = vec![0u8; w * h];
        let mut k = 0;
        for (ry, (start, len)) in diamond_rows(tmp.tile_width, tmp.tile_height)
            .iter()
            .enumerate()
        {
            for dx in 0..*len as usize {
                if let Some(&p) = cell.image.get(k) {
                    boxed[ry * w + *start as usize + dx] = p;
                }
                k += 1;
            }
        }
        let half = i32::try_from(w / 2).expect("small");
        pieces.push(Piece {
            order: (0, row, t.level),
            x: top.x - half,
            y: top.y,
            width: w,
            pixels: boxed,
            light: cell_light,
        });
        if let Some(e) = &cell.extra {
            pieces.push(Piece {
                order: (1, row, t.level),
                x: top.x - half + e.x - cell.x,
                y: top.y + e.y - cell.y,
                width: e.width as usize,
                pixels: e.image.clone(),
                light: cell_light,
            });
        }
    }
    pieces.sort_by_key(|p| p.order);
    let mut lit: BTreeMap<crate::lighting::Light, [Rgb; 256]> = BTreeMap::new();
    for p in pieces {
        let palette = lit
            .entry(p.light)
            .or_insert_with(|| crate::lighting::Lighting::lit(&lib.palette, p.light));
        canvas.blit(p.x, p.y, p.width, &p.pixels, palette, false);
    }
    stats
}

/// The world-pixel bounds of a map's cells, with a margin for raised cells
/// and extra art.
pub fn bounds(map: &Map) -> (i32, i32, i32, i32) {
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for t in &map.tiles {
        let p = Cell::new(t.x, t.y).screen(i32::from(t.level));
        (x0, y0, x1, y1) = (
            x0.min(p.x - 30),
            y0.min(p.y),
            x1.max(p.x + 30),
            y1.max(p.y + 30),
        );
    }
    if x0 > x1 {
        return (0, 0, 1, 1);
    }
    (x0, y0 - 120, x1, y1 + 30)
}
