//! A frame's sprites for the GPU: palette-indexed pixels packed into one
//! 8-bit atlas, the palettes they use (already lit) as rows of colours,
//! and a draw list in drawing order. The GPU looks each pixel's index up
//! in its row, so house colours and lighting stay what the palette says;
//! the CPU canvas ([`crate::scene::draw`]) is the reference it must match.

use std::collections::BTreeMap;

use crate::lighting::{Light, Lighting};
use crate::scene::{PaletteKind, Piece};

/// Width of the atlas; rows of sprites are packed into it top to bottom.
pub const ATLAS_WIDTH: u32 = 2048;

/// One sprite to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpriteDraw {
    /// Where its top-left goes, in world pixels.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Where its pixels are in the atlas.
    pub atlas_x: u32,
    pub atlas_y: u32,
    /// Its palette's row in [`Sprites::palettes`].
    pub palette: u32,
    /// A shadow: index 1 darkens what's beneath to half; other indices
    /// draw nothing.
    pub shadow: bool,
}

/// Everything the GPU needs to draw a frame's sprites.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sprites {
    /// 8-bit indices, `ATLAS_WIDTH` wide, `atlas_height` tall.
    pub atlas: Vec<u8>,
    pub atlas_height: u32,
    /// 256 RGBA colours per row.
    pub palettes: Vec<[u8; 4]>,
    pub draws: Vec<SpriteDraw>,
}

impl Sprites {
    /// Pack `pieces` (in any order; they're drawn by their `order`), with
    /// each palette lit by its piece's cell light, as [`crate::scene::draw`]
    /// does on the CPU.
    pub fn build(
        pieces: Vec<Piece>,
        palette: &dyn Fn(PaletteKind) -> [[u8; 3]; 256],
        light: &Lighting,
    ) -> Sprites {
        let mut pieces = pieces;
        pieces.sort_by_key(|p| p.order);
        let mut rows: BTreeMap<(PaletteKind, Light), u32> = BTreeMap::new();
        let mut out = Sprites::default();
        // Shelf packing: left to right, a new shelf when a row is full.
        let (mut x, mut y, mut shelf) = (0u32, 0u32, 0u32);
        for p in pieces {
            let width = u32::try_from(p.width).unwrap_or(0);
            if width == 0 || width > ATLAS_WIDTH || p.pixels.is_empty() {
                continue;
            }
            let height = u32::try_from(p.pixels.len() / p.width).unwrap_or(0);
            if x + width > ATLAS_WIDTH {
                (x, y, shelf) = (0, y + shelf, 0);
            }
            if out.atlas_height < y + height {
                out.atlas_height = y + height;
                out.atlas
                    .resize((ATLAS_WIDTH * out.atlas_height) as usize, 0);
            }
            for (row, line) in p.pixels.chunks(p.width).enumerate() {
                let at = ((y + u32::try_from(row).unwrap_or(0)) * ATLAS_WIDTH + x) as usize;
                out.atlas[at..at + line.len()].copy_from_slice(line);
            }
            let cell = light.cell(p.at.0, p.at.1, p.at.2);
            let next = u32::try_from(rows.len()).unwrap_or(u32::MAX);
            let row = *rows.entry((p.palette, cell)).or_insert_with(|| {
                let lit = Lighting::lit(&palette(p.palette), cell);
                out.palettes
                    .extend(lit.iter().map(|c| [c[0], c[1], c[2], 255]));
                next
            });
            out.draws.push(SpriteDraw {
                x: p.x,
                y: p.y,
                width,
                height,
                atlas_x: x,
                atlas_y: y,
                palette: row,
                shadow: p.shadow,
            });
            x += width;
            shelf = shelf.max(height);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(order: i32, width: usize, pixels: Vec<u8>, shadow: bool) -> Piece {
        Piece {
            order: (order, 0, 0),
            x: order,
            y: 0,
            width,
            pixels,
            palette: PaletteKind::Iso,
            shadow,
            at: (0, 0, 0),
        }
    }

    #[test]
    fn pieces_pack_in_draw_order_with_shared_palette_rows() {
        let grey = |_: PaletteKind| [[100u8, 100, 100]; 256];
        let s = Sprites::build(
            vec![
                piece(5, 2, vec![1, 2, 3, 4], false),
                piece(1, 3, vec![1, 1, 1], true),
            ],
            &grey,
            &Lighting::default(),
        );
        assert_eq!(s.draws.len(), 2);
        assert_eq!(
            (s.draws[0].x, s.draws[0].shadow),
            (1, true),
            "sorted by order"
        );
        assert_eq!((s.draws[1].atlas_x, s.draws[1].height), (3, 2));
        assert_eq!(s.palettes.len(), 256, "one lit palette shared");
        assert_eq!(&s.atlas[3..5], &[1, 2]);
        assert_eq!(&s.atlas[2048 + 3..2048 + 5], &[3, 4]);
    }
}
