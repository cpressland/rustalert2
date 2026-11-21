//! Shroud: black over what the player hasn't seen, with soft edges drawn
//! from `shroud.shp` (see `docs/spec/shroud.md`).
//!
//! Each seen cell takes an edge shape from the eight neighbours it can't
//! see, through a 256-entry table in `gamemd.exe`. The shapes hold
//! brightness (2 is dark, 127 clear), written into a buffer that then
//! darkens the frame.

use std::collections::BTreeSet;

use ra2_formats::map::Map;
use ra2_formats::pe::Pe;
use ra2_formats::shp::Shp;
use ra2_sim::coords::Cell;

use crate::canvas::Canvas;

/// Where `gamemd.exe` 1.001 keeps the neighbour-mask to frame table.
const FRAME_TABLE: u32 = 0x007F_4194;
/// The frame drawn over a cell the player hasn't seen.
pub const SOLID: usize = 15;
/// Brightness meaning "not darkened".
pub const CLEAR: u8 = 127;
/// `shroud.shp`'s transparent index.
const SKIP: u8 = 0xFE;

/// The bit each unseen neighbour adds to a cell's mask, by (dx, dy).
pub const NEIGHBOURS: [((i16, i16), u8); 8] = [
    ((-1, -1), 0x40),
    ((0, -1), 0x80),
    ((1, -1), 0x01),
    ((-1, 0), 0x20),
    ((1, 0), 0x02),
    ((-1, 1), 0x10),
    ((0, 1), 0x08),
    ((1, 1), 0x04),
];

/// The neighbour-mask table: a frame for each mask, or −1 (nothing to
/// draw: frame 0, which is clear) or −2 (the solid frame).
#[derive(Debug, Clone)]
pub struct FrameTable([i8; 256]);

impl FrameTable {
    pub fn from_gamemd(exe: &[u8]) -> Option<FrameTable> {
        let pe = Pe::parse(exe)?;
        let bytes = pe.read(FRAME_TABLE, 256)?;
        let mut t = [0i8; 256];
        for (o, &b) in t.iter_mut().zip(bytes) {
            *o = i8::from_ne_bytes([b]);
        }
        // Sanity: every entry is a frame of the 48-shape half, or −1/−2.
        t.iter()
            .all(|&v| (-2..48).contains(&v))
            .then_some(FrameTable(t))
    }

    /// The frame for a seen cell with `mask` of unseen neighbours.
    pub fn frame(&self, mask: u8) -> usize {
        match self.0[usize::from(mask)] {
            -2 => SOLID,
            v => usize::try_from(v).unwrap_or(0),
        }
    }
}

/// The mask of a cell's unseen neighbours.
pub fn mask(seen: &BTreeSet<(i16, i16)>, x: i16, y: i16) -> u8 {
    NEIGHBOURS
        .iter()
        .filter(|((dx, dy), _)| !seen.contains(&(x + dx, y + dy)))
        .fold(0, |m, (_, bit)| m | bit)
}

/// `shroud.shp`'s frames, each a 60×30 brightness image.
#[derive(Debug, Clone)]
pub struct Shapes(Vec<Vec<u8>>);

impl Shapes {
    pub fn parse(data: &[u8]) -> Option<Shapes> {
        let shp = Shp::parse(data).ok()?;
        if (shp.width, shp.height) != (60, 30) || shp.frames.len() < 48 {
            return None;
        }
        (0..48)
            .map(|i| shp.frame_on_canvas(i).ok())
            .collect::<Option<Vec<_>>>()
            .map(Shapes)
    }
}

/// Darken `canvas` by the shroud over every cell `seen` doesn't hold.
/// Shapes are drawn at each cell's height into a brightness buffer, back to
/// front, the last one written winning, as the game's buffer does.
pub fn draw(
    canvas: &mut Canvas,
    map: &Map,
    seen: &BTreeSet<(i16, i16)>,
    table: &FrameTable,
    shapes: &Shapes,
) {
    let (w, h) = (canvas.width as usize, canvas.height as usize);
    let mut light = vec![CLEAR; w * h];
    let mut cells: Vec<_> = map.tiles.iter().collect();
    cells.sort_by_key(|t| (i32::from(t.x) + i32::from(t.y), t.x));
    for t in cells {
        let frame = if seen.contains(&(t.x, t.y)) {
            table.frame(mask(seen, t.x, t.y))
        } else {
            SOLID
        };
        if frame == 0 {
            continue;
        }
        let top = Cell::new(t.x, t.y).screen(i32::from(t.level));
        let (x0, y0) = (top.x - 30 - canvas.origin_x, top.y - canvas.origin_y);
        for (dy, row) in (0i32..).zip(shapes.0[frame].chunks(60)) {
            for (dx, &v) in (0i32..).zip(row) {
                if v == SKIP {
                    continue;
                }
                if let (Ok(px), Ok(py)) = (usize::try_from(x0 + dx), usize::try_from(y0 + dy))
                    && px < w
                    && py < h
                {
                    light[py * w + px] = v.min(CLEAR);
                }
            }
        }
    }
    for (px, &l) in canvas.rgba.as_chunks_mut::<4>().0.iter_mut().zip(&light) {
        if l < CLEAR {
            for c in &mut px[..3] {
                *c =
                    u8::try_from(u16::from(*c) * u16::from(l) / u16::from(CLEAR)).expect("smaller");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_sets_a_bit_per_unseen_neighbour() {
        let mut seen = BTreeSet::new();
        for y in -1..=1 {
            for x in -1..=1 {
                seen.insert((x, y));
            }
        }
        assert_eq!(mask(&seen, 0, 0), 0);
        seen.remove(&(0, -1));
        seen.remove(&(1, 1));
        assert_eq!(mask(&seen, 0, 0), 0x80 | 0x04);
    }

    #[test]
    fn table_maps_minus_two_to_solid_and_minus_one_to_clear() {
        let mut t = [3i8; 256];
        t[0] = -1;
        t[0xFF] = -2;
        let t = FrameTable(t);
        assert_eq!(t.frame(0), 0);
        assert_eq!(t.frame(0xFF), SOLID);
        assert_eq!(t.frame(7), 3);
    }
}
