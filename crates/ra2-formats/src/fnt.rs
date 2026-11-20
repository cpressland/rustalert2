//! The game's Unicode bitmap font (`game.fnt`). See `docs/spec/font.md`.

use std::fmt;

const HEADER_LEN: usize = 28;
const MAP_LEN: usize = 65_536 * 2;

/// Why a font could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotFont(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotFont(why) => write!(f, "not a fonT font: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// A parsed font. Glyphs are read from the file's bytes on demand.
#[derive(Debug, Clone)]
pub struct Font<'a> {
    /// The widest glyph, in pixels.
    pub max_width: u32,
    /// Bytes per glyph row (1 bit per pixel).
    pub stride: u32,
    /// Rows stored per glyph.
    pub lines: u32,
    /// The line height to lay text out with.
    pub height: u32,
    pub glyph_count: u32,
    glyph_size: usize,
    data: &'a [u8],
}

/// One glyph: its advance width and a row-major mask (`width × lines`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub width: u8,
    pub mask: Vec<bool>,
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

impl<'a> Font<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Font<'a>, Error> {
        if data.len() < HEADER_LEN + MAP_LEN || &data[..4] != b"fonT" {
            return Err(Error::NotFont("no fonT header"));
        }
        let (max_width, stride, lines, height, glyph_count, glyph_size) = (
            u32_at(data, 4),
            u32_at(data, 8),
            u32_at(data, 12),
            u32_at(data, 16),
            u32_at(data, 20),
            u32_at(data, 24),
        );
        let to = |v: u32| usize::try_from(v).unwrap_or(usize::MAX);
        if to(glyph_size) != 1 + to(stride) * to(lines) {
            return Err(Error::NotFont("glyph size isn't 1 + stride × lines"));
        }
        if to(stride) * 8 < to(max_width) {
            return Err(Error::NotFont("stride too narrow for the widest glyph"));
        }
        if data.len() != HEADER_LEN + MAP_LEN + to(glyph_count) * to(glyph_size) {
            return Err(Error::NotFont("size doesn't match the glyph count"));
        }
        Ok(Font {
            max_width,
            stride,
            lines,
            height,
            glyph_count,
            glyph_size: to(glyph_size),
            data,
        })
    }

    /// The glyph index for a character, if the font maps it. The map is
    /// 1-based: 0 means no glyph, and *k* means glyph *k* − 1.
    pub fn index(&self, c: char) -> Option<u16> {
        let code = u32::from(c);
        let code = usize::try_from(code).ok().filter(|&c| c < 65_536)?;
        let at = HEADER_LEN + code * 2;
        let i = u16::from_le_bytes([self.data[at], self.data[at + 1]]);
        i.checked_sub(1)
    }

    /// A glyph by index. Pixels are 1 bit each, most significant bit first.
    pub fn glyph(&self, index: u16) -> Option<Glyph> {
        if u32::from(index) >= self.glyph_count {
            return None;
        }
        let start = HEADER_LEN + MAP_LEN + usize::from(index) * self.glyph_size;
        let g = &self.data[start..start + self.glyph_size];
        let width = g[0];
        let stride =
            self.glyph_size.saturating_sub(1) / usize::try_from(self.lines).unwrap_or(1).max(1);
        let lines = usize::try_from(self.lines).unwrap_or(0);
        let mut mask = Vec::with_capacity(usize::from(width) * lines);
        for row in 0..lines {
            let start = 1 + row * stride;
            let bytes = &g[start..start + stride];
            for x in 0..usize::from(width) {
                mask.push(bytes.get(x / 8).is_some_and(|b| b & (0x80 >> (x % 8)) != 0));
            }
        }
        Some(Glyph { width, mask })
    }

    /// How many characters the font maps.
    pub fn mapped(&self) -> usize {
        self.data[HEADER_LEN..HEADER_LEN + MAP_LEN]
            .as_chunks::<2>()
            .0
            .iter()
            .filter(|c| **c != [0, 0])
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Vec<u8> {
        let mut d = b"fonT".to_vec();
        for v in [8u32, 1, 2, 3, 2, 3] {
            d.extend_from_slice(&v.to_le_bytes());
        }
        let mut map = vec![0u8; MAP_LEN];
        map[usize::from(b'A') * 2] = 2;
        d.extend_from_slice(&map);
        d.extend_from_slice(&[0, 0, 0]);
        d.extend_from_slice(&[3, 0b1010_0000, 0b0100_0000]);
        d
    }

    #[test]
    fn glyphs() {
        let data = font();
        let f = Font::parse(&data).unwrap();
        assert_eq!(
            (f.max_width, f.stride, f.lines, f.height, f.glyph_count),
            (8, 1, 2, 3, 2)
        );
        assert_eq!(f.mapped(), 1);
        let g = f.glyph(f.index('A').unwrap()).unwrap();
        assert_eq!(g.width, 3);
        assert_eq!(g.mask, [true, false, true, false, true, false]);
        assert_eq!(f.index('B'), None);
        assert_eq!(f.glyph(2), None);
        let mut bad = data.clone();
        bad.pop();
        assert!(Font::parse(&bad).is_err());
    }
}
