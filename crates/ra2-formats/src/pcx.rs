//! PCX images (UI art: loading screens, menus). See `docs/spec/font.md`.

use std::fmt;

/// Why an image could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotPcx(&'static str),
    Truncated,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotPcx(why) => write!(f, "not a supported PCX: {why}"),
            Error::Truncated => f.write_str("PCX data is truncated"),
        }
    }
}

impl std::error::Error for Error {}

/// A decoded image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Image {
    /// 8-bit indices with the file's 256-colour palette (8-bit components).
    Indexed {
        width: u16,
        height: u16,
        pixels: Vec<u8>,
        palette: Vec<[u8; 3]>,
    },
    /// 24-bit colour.
    Rgb {
        width: u16,
        height: u16,
        pixels: Vec<[u8; 3]>,
    },
}

impl Image {
    pub fn size(&self) -> (u16, u16) {
        match self {
            Image::Indexed { width, height, .. } | Image::Rgb { width, height, .. } => {
                (*width, *height)
            }
        }
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// Decode an 8-bit PCX, one plane (paletted) or three (RGB).
pub fn decode(data: &[u8]) -> Result<Image, Error> {
    if data.len() < 128 || data[0] != 0x0a || data[2] != 1 {
        return Err(Error::NotPcx("no RLE PCX header"));
    }
    if data[3] != 8 {
        return Err(Error::NotPcx("not 8 bits per plane"));
    }
    let (x0, y0, x1, y1) = (
        u16_at(data, 4),
        u16_at(data, 6),
        u16_at(data, 8),
        u16_at(data, 10),
    );
    if x1 < x0 || y1 < y0 {
        return Err(Error::NotPcx("bad window"));
    }
    let (width, height) = (x1 - x0 + 1, y1 - y0 + 1);
    let planes = usize::from(data[65]);
    let bpl = usize::from(u16_at(data, 66));
    if !(planes == 1 || planes == 3) || bpl < usize::from(width) {
        return Err(Error::NotPcx("unsupported planes or line length"));
    }
    // RLE: a byte with its top two bits set repeats the next byte.
    let total = bpl * planes * usize::from(height);
    let mut raw = Vec::with_capacity(total);
    let mut at = 128;
    while raw.len() < total {
        let b = *data.get(at).ok_or(Error::Truncated)?;
        at += 1;
        if b >= 0xc0 {
            let v = *data.get(at).ok_or(Error::Truncated)?;
            at += 1;
            raw.extend(std::iter::repeat_n(v, usize::from(b & 0x3f)));
        } else {
            raw.push(b);
        }
    }
    raw.truncate(total);
    let w = usize::from(width);
    if planes == 1 {
        let tail = data.len().checked_sub(769).ok_or(Error::Truncated)?;
        if data[tail] != 12 {
            return Err(Error::NotPcx("no 256-colour palette"));
        }
        let palette = data[tail + 1..].as_chunks::<3>().0.to_vec();
        let pixels = raw
            .chunks(bpl)
            .flat_map(|row| row[..w].iter().copied())
            .collect();
        Ok(Image::Indexed {
            width,
            height,
            pixels,
            palette,
        })
    } else {
        let mut pixels = Vec::with_capacity(w * usize::from(height));
        for row in raw.chunks(bpl * 3) {
            for x in 0..w {
                pixels.push([row[x], row[bpl + x], row[2 * bpl + x]]);
            }
        }
        Ok(Image::Rgb {
            width,
            height,
            pixels,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: u16, h: u16, planes: u8, bpl: u16) -> Vec<u8> {
        let mut d = vec![0u8; 128];
        d[0] = 0x0a;
        d[1] = 5;
        d[2] = 1;
        d[3] = 8;
        d[8..10].copy_from_slice(&(w - 1).to_le_bytes());
        d[10..12].copy_from_slice(&(h - 1).to_le_bytes());
        d[65] = planes;
        d[66..68].copy_from_slice(&bpl.to_le_bytes());
        d
    }

    #[test]
    fn indexed_and_rgb() {
        let mut d = header(3, 2, 1, 4);
        d.extend_from_slice(&[0xc4, 7, 1, 2, 0xc0 | 1, 0xc5, 3]);
        d.push(12);
        d.extend((0..=255u8).flat_map(|i| [i, i, i]));
        match decode(&d).unwrap() {
            Image::Indexed {
                width,
                height,
                pixels,
                palette,
            } => {
                assert_eq!((width, height), (3, 2));
                assert_eq!(pixels, [7, 7, 7, 1, 2, 0xc5]);
                assert_eq!(palette[9], [9, 9, 9]);
            }
            Image::Rgb { .. } => panic!("expected indexed"),
        }
        let mut d = header(2, 1, 3, 2);
        d.extend_from_slice(&[1, 2, 3, 4, 5, 6]);
        match decode(&d).unwrap() {
            Image::Rgb { pixels, .. } => assert_eq!(pixels, [[1, 3, 5], [2, 4, 6]]),
            Image::Indexed { .. } => panic!("expected rgb"),
        }
        assert!(decode(&d[..130]).is_err());
    }
}
