//! SHP sprites, *Tiberian Sun* / *Red Alert 2* flavour. See `docs/spec/shp.md`.
//!
//! A file is a canvas size and a list of frames. Each frame is a rectangle
//! placed on that canvas, stored raw or with transparent runs compressed.
//! Index 0 is transparent.

use std::fmt;

const HEADER_LEN: usize = 8;
const FRAME_HEADER_LEN: usize = 24;
const MAX_CANVAS_PIXELS: u32 = 4096 * 4096;
/// A two-byte zero run covers at most 255 pixels, plus overshoot.
const MAX_EXPANSION: usize = 128;

/// How a frame's pixels are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// `w × h` bytes, row by row.
    Raw,
    /// Each row is a `u16` length (including itself) then the row's bytes.
    Rows,
    /// Each row is a `u16` length (including itself) then bytes in which
    /// `0, n` stands for `n` transparent pixels.
    RowsZeroRuns,
}

/// One frame's header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    /// The flags byte as stored. Bit 1 means "rows with lengths", and with
    /// it, bit 0 means "zero runs".
    pub flags: u8,
    /// The frame's colour on the radar, as stored (R, G, B, unused).
    pub radar_color: [u8; 4],
    /// Where the frame's data starts in the file; 0 for an empty frame.
    pub offset: u32,
}

impl FrameHeader {
    pub fn compression(&self) -> Compression {
        match (self.flags & 2 != 0, self.flags & 1 != 0) {
            (false, _) => Compression::Raw,
            (true, false) => Compression::Rows,
            (true, true) => Compression::RowsZeroRuns,
        }
    }

    /// No pixels: no data, or no area.
    pub fn is_empty(&self) -> bool {
        self.offset == 0 || self.width == 0 || self.height == 0
    }
}

/// Why a file or frame could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotShp(&'static str),
    Truncated,
    /// A frame's rows didn't add up: the data is damaged or not this format.
    BadRow {
        frame: usize,
        row: usize,
    },
    NoSuchFrame(usize),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotShp(why) => write!(f, "not a TS/RA2 SHP: {why}"),
            Error::Truncated => f.write_str("SHP is truncated"),
            Error::BadRow { frame, row } => {
                write!(f, "frame {frame}, row {row}: row data doesn't fit")
            }
            Error::NoSuchFrame(i) => write!(f, "no frame {i}"),
        }
    }
}

impl std::error::Error for Error {}

/// A decoded frame: its rectangle on the canvas and its palette indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

/// A parsed SHP. Frames are decoded on demand.
#[derive(Debug, Clone)]
pub struct Shp<'a> {
    pub width: u16,
    pub height: u16,
    pub frames: Vec<FrameHeader>,
    data: &'a [u8],
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().expect("4 bytes"))
}

impl<'a> Shp<'a> {
    /// Parse the header and frame table, checking that every frame lies on
    /// the canvas and its data starts inside the file.
    pub fn parse(data: &'a [u8]) -> Result<Shp<'a>, Error> {
        if data.len() < HEADER_LEN {
            return Err(Error::NotShp("shorter than a header"));
        }
        if u16_at(data, 0) != 0 {
            return Err(Error::NotShp("first word isn't 0"));
        }
        let (width, height, count) = (
            u16_at(data, 2),
            u16_at(data, 4),
            usize::from(u16_at(data, 6)),
        );
        if count == 0 {
            return Err(Error::NotShp("no frames"));
        }
        // Far larger than any sprite the game draws; stops a forged header
        // from asking for gigabytes of canvas.
        if u32::from(width) * u32::from(height) > MAX_CANVAS_PIXELS {
            return Err(Error::NotShp("canvas larger than 4096 x 4096"));
        }
        let table_end = HEADER_LEN + count * FRAME_HEADER_LEN;
        if data.len() < table_end {
            return Err(Error::NotShp("frame table runs past the end"));
        }
        let mut frames = Vec::with_capacity(count);
        for i in 0..count {
            let h = &data[HEADER_LEN + i * FRAME_HEADER_LEN..];
            let f = FrameHeader {
                x: u16_at(h, 0),
                y: u16_at(h, 2),
                width: u16_at(h, 4),
                height: u16_at(h, 6),
                flags: h[8],
                radar_color: [h[12], h[13], h[14], h[15]],
                offset: u32_at(h, 20),
            };
            if !f.is_empty() {
                if u32::from(f.x) + u32::from(f.width) > u32::from(width)
                    || u32::from(f.y) + u32::from(f.height) > u32::from(height)
                {
                    return Err(Error::NotShp("a frame lies off the canvas"));
                }
                let off = usize::try_from(f.offset).map_err(|_| Error::Truncated)?;
                if off < table_end || off >= data.len() {
                    return Err(Error::NotShp("a frame's data is outside the file"));
                }
            }
            frames.push(f);
        }
        Ok(Shp {
            width,
            height,
            frames,
            data,
        })
    }

    /// Decode one frame's pixels.
    pub fn frame(&self, index: usize) -> Result<Frame, Error> {
        let h = *self.frames.get(index).ok_or(Error::NoSuchFrame(index))?;
        if h.is_empty() {
            // Nothing to draw, whatever size the header claims.
            return Ok(Frame {
                x: h.x,
                y: h.y,
                width: 0,
                height: 0,
                pixels: Vec::new(),
            });
        }
        let (w, rows) = (usize::from(h.width), usize::from(h.height));
        let mut at = usize::try_from(h.offset).map_err(|_| Error::Truncated)?;
        // The frame's data can't expand to more than this.
        if w * rows > (self.data.len() - at).saturating_mul(MAX_EXPANSION) {
            return Err(Error::Truncated);
        }
        let mut pixels = vec![0u8; w * rows];
        let data = self.data;
        match h.compression() {
            Compression::Raw => {
                let src = data.get(at..at + w * rows).ok_or(Error::Truncated)?;
                pixels.copy_from_slice(src);
            }
            Compression::Rows | Compression::RowsZeroRuns => {
                let runs = h.compression() == Compression::RowsZeroRuns;
                for row in 0..rows {
                    let len = usize::from(u16::from_le_bytes(
                        data.get(at..at + 2)
                            .ok_or(Error::Truncated)?
                            .try_into()
                            .expect("2 bytes"),
                    ));
                    let body = data
                        .get(at + 2..at + len)
                        .ok_or(Error::BadRow { frame: index, row })?;
                    let out = &mut pixels[row * w..(row + 1) * w];
                    if runs {
                        decode_zero_runs(body, out).ok_or(Error::BadRow { frame: index, row })?;
                    } else {
                        if body.len() != w {
                            return Err(Error::BadRow { frame: index, row });
                        }
                        out.copy_from_slice(body);
                    }
                    at += len;
                }
            }
        }
        Ok(Frame {
            x: h.x,
            y: h.y,
            width: h.width,
            height: h.height,
            pixels,
        })
    }

    /// One frame drawn onto a transparent canvas of the file's size.
    pub fn frame_on_canvas(&self, index: usize) -> Result<Vec<u8>, Error> {
        let f = self.frame(index)?;
        let cw = usize::from(self.width);
        let mut canvas = vec![0u8; cw * usize::from(self.height)];
        // An empty frame draws nothing. Its rectangle isn't checked against
        // the canvas at parse time, so it mustn't be used here.
        if self.frames[index].is_empty() {
            return Ok(canvas);
        }
        for row in 0..usize::from(f.height) {
            let dst = (usize::from(f.y) + row) * cw + usize::from(f.x);
            let src = row * usize::from(f.width);
            canvas[dst..dst + usize::from(f.width)]
                .copy_from_slice(&f.pixels[src..src + usize::from(f.width)]);
        }
        Ok(canvas)
    }
}

/// Expand one zero-run row into `out`. Every byte of `body` must be used,
/// literal pixels must lie inside the row, and the row must be covered. A
/// run of transparent pixels may run past the row's end: the shipped files
/// often end a row with a run one or more pixels too long, and transparent
/// pixels past the edge draw nothing.
fn decode_zero_runs(body: &[u8], out: &mut [u8]) -> Option<()> {
    let (mut i, mut x) = (0, 0);
    while i < body.len() {
        if body[i] == 0 {
            let n = usize::from(*body.get(i + 1)?);
            // Already zero: just advance.
            x += n;
            i += 2;
        } else {
            *out.get_mut(x)? = body[i];
            x += 1;
            i += 1;
        }
    }
    (x >= out.len()).then_some(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Our own encoder, for fixtures: zero-run compression when `runs`.
    pub(crate) fn encode(
        width: u16,
        height: u16,
        frames: &[(u16, u16, u16, u16, Vec<u8>)],
        flags: u8,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        for v in [0u16, width, height, u16::try_from(frames.len()).unwrap()] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let mut data = Vec::new();
        let table_end = 8 + frames.len() * 24;
        let mut headers = Vec::new();
        for (x, y, w, h, px) in frames {
            let offset = if px.is_empty() {
                0
            } else {
                u32::try_from(table_end + data.len()).unwrap()
            };
            if !px.is_empty() {
                for row in px.chunks(usize::from(*w)) {
                    let body: Vec<u8> = if flags == 3 {
                        let mut b = Vec::new();
                        let mut i = 0;
                        while i < row.len() {
                            if row[i] == 0 {
                                let n = row[i..].iter().take_while(|&&p| p == 0).count().min(255);
                                b.extend_from_slice(&[0, u8::try_from(n).unwrap()]);
                                i += n;
                            } else {
                                b.push(row[i]);
                                i += 1;
                            }
                        }
                        b
                    } else {
                        row.to_vec()
                    };
                    if flags & 2 != 0 {
                        data.extend_from_slice(
                            &u16::try_from(body.len() + 2).unwrap().to_le_bytes(),
                        );
                    }
                    data.extend_from_slice(&body);
                }
            }
            let mut hdr = Vec::new();
            for v in [*x, *y, *w, *h] {
                hdr.extend_from_slice(&v.to_le_bytes());
            }
            hdr.extend_from_slice(&[flags, 0, 0, 0, 1, 2, 3, 0, 0, 0, 0, 0]);
            hdr.extend_from_slice(&offset.to_le_bytes());
            headers.extend_from_slice(&hdr);
        }
        out.extend_from_slice(&headers);
        out.extend_from_slice(&data);
        out
    }

    fn sample() -> Vec<u8> {
        vec![0, 0, 5, 0, 7, 7, 0, 0, 0, 1]
    }

    #[test]
    fn every_compression_round_trips() {
        for flags in [1u8, 2, 3] {
            let file = encode(8, 4, &[(1, 1, 5, 2, sample()), (0, 0, 0, 0, vec![])], flags);
            let shp = Shp::parse(&file).unwrap();
            assert_eq!(shp.frames.len(), 2);
            let f = shp.frame(0).unwrap();
            assert_eq!(f.pixels, sample(), "flags {flags}");
            assert!(shp.frames[1].is_empty());
            assert_eq!(shp.frame(1).unwrap().pixels, Vec::<u8>::new());
            assert_eq!(shp.frame(1).unwrap().width, 0);
            let canvas = shp.frame_on_canvas(0).unwrap();
            assert_eq!(&canvas[8 + 1..8 + 6], &sample()[..5]);
            assert_eq!(shp.frames[0].radar_color, [1, 2, 3, 0]);
        }
    }

    #[test]
    fn rejects_damage() {
        assert!(matches!(
            Shp::parse(b"\x01\x00\x08\x00\x08\x00\x01\x00"),
            Err(Error::NotShp(_))
        ));
        let mut file = encode(8, 4, &[(1, 1, 5, 2, sample())], 3);
        // A frame off the canvas.
        let mut bad = file.clone();
        bad[8] = 7;
        assert!(Shp::parse(&bad).is_err());
        // A trailing run that overshoots the row is tolerated, as in the
        // shipped files.
        let n = file.len();
        file[n - 1] = 200;
        let shp = Shp::parse(&file).unwrap();
        assert!(shp.frame(0).is_ok());
        // A literal pixel past the row's end is not.
        let mut file = encode(
            8,
            4,
            &[(1, 1, 5, 2, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])],
            3,
        );
        let n = file.len();
        let row_len = 2 + 5;
        file[n - row_len..n - row_len + 2].copy_from_slice(&8u16.to_le_bytes());
        file.push(11);
        let shp = Shp::parse(&file).unwrap();
        assert!(matches!(shp.frame(0), Err(Error::BadRow { .. })));
        assert_eq!(shp.frame(9).unwrap_err(), Error::NoSuchFrame(9));
        // Found by fuzzing: an empty frame whose rectangle lies off the canvas.
        let mut empty_off = encode(4, 4, &[(0, 0, 0, 0, vec![])], 1);
        empty_off[8 + 4..8 + 6].copy_from_slice(&200u16.to_le_bytes());
        empty_off[8 + 6..8 + 8].copy_from_slice(&3u16.to_le_bytes());
        let shp = Shp::parse(&empty_off).unwrap();
        assert_eq!(shp.frame_on_canvas(0).unwrap(), vec![0; 16]);
        // Found by fuzzing: an empty frame claiming 65280 x 65535 pixels
        // allocated 4 GB.
        let mut huge = encode(49, 1, &[(0, 0, 0, 0, vec![])], 1);
        huge[8 + 4..8 + 8].copy_from_slice(&[0, 255, 255, 255]);
        assert_eq!(Shp::parse(&huge).unwrap().frame(0).unwrap().pixels.len(), 0);
        // And a canvas no sprite has.
        assert!(Shp::parse(&encode(5000, 5000, &[(0, 0, 0, 0, vec![])], 1)).is_err());
    }

    #[test]
    fn noise_never_panics() {
        let mut seed = 7u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            seed.to_le_bytes()[2]
        };
        let base = encode(
            8,
            4,
            &[(1, 1, 5, 2, sample()), (0, 0, 8, 4, vec![3; 32])],
            3,
        );
        for _ in 0..2000 {
            let mut f = base.clone();
            let i = usize::from(next()) % f.len();
            f[i] = next();
            if let Ok(shp) = Shp::parse(&f) {
                for k in 0..shp.frames.len() {
                    let _ = shp.frame(k);
                }
            }
        }
    }
}
