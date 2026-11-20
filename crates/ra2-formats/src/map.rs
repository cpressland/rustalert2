//! Scenario files (`.map`, `.yrm`, `.mpr`): an INI whose packed sections
//! hold the terrain, overlays and preview. See `docs/spec/map.md`.
//!
//! This reads the packed parts. The object, trigger and other sections stay
//! in the INI for the systems that read them (M4, M7).

use std::fmt;

use crate::codec::{self, Compression, base64, unpack_blocks};
use crate::ini::{Ini, atoi};

/// The overlay grid is always 512 × 512 cells.
pub const OVERLAY_SIZE: usize = 512 * 512;
const ISO_TILE_LEN: usize = 11;

/// Why a map could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NoMapSection,
    BadSize(String),
    Pack {
        section: &'static str,
        error: codec::Error,
    },
    BadLength {
        section: &'static str,
        expected: usize,
        got: usize,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NoMapSection => f.write_str("no [Map] Size"),
            Error::BadSize(v) => write!(f, "bad [Map] Size: {v}"),
            Error::Pack { section, error } => write!(f, "[{section}]: {error}"),
            Error::BadLength {
                section,
                expected,
                got,
            } => {
                write!(
                    f,
                    "[{section}] unpacked to {got} bytes, expected {expected}"
                )
            }
        }
    }
}

impl std::error::Error for Error {}

/// A rectangle as maps write it: `x,y,width,height`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn parse(v: &str) -> Option<Rect> {
        let p: Vec<i32> = v.split(',').map(atoi).collect();
        (p.len() == 4).then(|| Rect {
            x: p[0],
            y: p[1],
            width: p[2],
            height: p[3],
        })
    }
}

/// One terrain cell from `[IsoMapPack5]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsoTile {
    pub x: i16,
    pub y: i16,
    /// The tile number, counting through the theater's tile sets; `-1` and
    /// `0xFFFF` both appear for "clear".
    pub tile: i32,
    /// Which cell of a multi-cell template this is.
    pub subtile: u8,
    pub level: u8,
    pub ice: u8,
}

/// A map's packed data, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Map {
    pub size: Rect,
    pub local_size: Option<Rect>,
    pub theater: String,
    pub tiles: Vec<IsoTile>,
    /// Bytes after the last whole tile in `[IsoMapPack5]`. Shipped maps end
    /// the pack with 4 such bytes.
    pub iso_trailing: Vec<u8>,
    /// Overlay type per cell (`0xFF` for none), indexed `y * 512 + x`.
    pub overlay: Option<Vec<u8>>,
    /// Overlay frame or growth stage per cell.
    pub overlay_data: Option<Vec<u8>>,
    /// The preview's size and RGB pixels, if the map has one.
    pub preview: Option<(Rect, Vec<u8>)>,
}

/// Concatenate a packed section's values in key order (`1`, `2`, ...).
fn packed_text(ini: &Ini, section: &str) -> Option<String> {
    let s = ini.section(section)?;
    let mut lines: Vec<(i32, &str)> = s.entries().map(|(k, v)| (atoi(k), v)).collect();
    lines.sort_by_key(|&(k, _)| k);
    Some(lines.into_iter().map(|(_, v)| v).collect())
}

fn unpack(
    ini: &Ini,
    section: &'static str,
    compression: Compression,
) -> Result<Option<Vec<u8>>, Error> {
    let Some(text) = packed_text(ini, section) else {
        return Ok(None);
    };
    let raw = base64(&text).map_err(|error| Error::Pack { section, error })?;
    unpack_blocks(&raw, compression)
        .map(Some)
        .map_err(|error| Error::Pack { section, error })
}

impl Map {
    /// Decode a map's packed sections from its INI.
    pub fn from_ini(ini: &Ini) -> Result<Map, Error> {
        let size_text = ini.get("Map", "Size").ok_or(Error::NoMapSection)?;
        let size = Rect::parse(size_text).ok_or_else(|| Error::BadSize(size_text.to_string()))?;
        let local_size = ini.get("Map", "LocalSize").and_then(Rect::parse);
        let theater = ini.get("Map", "Theater").unwrap_or_default().to_string();

        let raw = unpack(ini, "IsoMapPack5", Compression::Lzo)?.unwrap_or_default();
        let (whole, rest) = raw.as_chunks::<ISO_TILE_LEN>();
        let iso_trailing = rest.to_vec();
        let tiles = whole
            .iter()
            .map(|b| IsoTile {
                x: i16::from_le_bytes([b[0], b[1]]),
                y: i16::from_le_bytes([b[2], b[3]]),
                tile: i32::from_le_bytes([b[4], b[5], b[6], b[7]]),
                subtile: b[8],
                level: b[9],
                ice: b[10],
            })
            .collect();

        let overlay_pack = |section| -> Result<Option<Vec<u8>>, Error> {
            let out = unpack(ini, section, Compression::Lcw)?;
            if let Some(o) = &out
                && o.len() != OVERLAY_SIZE
            {
                return Err(Error::BadLength {
                    section,
                    expected: OVERLAY_SIZE,
                    got: o.len(),
                });
            }
            Ok(out)
        };
        let overlay = overlay_pack("OverlayPack")?;
        let overlay_data = overlay_pack("OverlayDataPack")?;

        let preview = match (
            ini.get("Preview", "Size").and_then(Rect::parse),
            unpack(ini, "PreviewPack", Compression::Lzo)?,
        ) {
            (Some(r), Some(px)) => {
                let expected = usize::try_from(r.width).unwrap_or(0)
                    * usize::try_from(r.height).unwrap_or(0)
                    * 3;
                if px.len() != expected {
                    return Err(Error::BadLength {
                        section: "PreviewPack",
                        expected,
                        got: px.len(),
                    });
                }
                Some((r, px))
            }
            _ => None,
        };

        Ok(Map {
            size,
            local_size,
            theater,
            tiles,
            iso_trailing,
            overlay,
            overlay_data,
            preview,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    /// Base64-encode for fixtures.
    fn b64(data: &[u8]) -> String {
        const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for c in data.chunks(3) {
            let n = c
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, &b)| n | (u32::from(b) << (16 - 8 * i)));
            for i in 0..=c.len() {
                out.push(char::from(
                    A[usize::try_from((n >> (18 - 6 * i)) & 63).unwrap()],
                ));
            }
        }
        out
    }

    /// LCW that stores `data` as literal runs, in blocks.
    fn lcw_blocks(data: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        for block in data.chunks(8192) {
            let mut packed = Vec::new();
            for run in block.chunks(63) {
                packed.push(0x80 | u8::try_from(run.len()).unwrap());
                packed.extend_from_slice(run);
            }
            packed.push(0x80);
            out.extend_from_slice(&u16::try_from(packed.len()).unwrap().to_le_bytes());
            out.extend_from_slice(&u16::try_from(block.len()).unwrap().to_le_bytes());
            out.extend_from_slice(&packed);
        }
        out
    }

    #[test]
    fn overlay_and_size() {
        let mut overlay = vec![0xffu8; OVERLAY_SIZE];
        overlay[512 * 3 + 7] = 5;
        let text = b64(&lcw_blocks(&overlay));
        let mut ini = String::from("[Map]\nSize=0,0,50,40\nTheater=TEMPERATE\n[OverlayPack]\n");
        for (i, line) in text.as_bytes().chunks(70).enumerate() {
            writeln!(ini, "{}={}", i + 1, std::str::from_utf8(line).unwrap()).unwrap();
        }
        let map = Map::from_ini(&Ini::parse(ini.as_bytes())).unwrap();
        assert_eq!(
            map.size,
            Rect {
                x: 0,
                y: 0,
                width: 50,
                height: 40
            }
        );
        assert_eq!(map.theater, "TEMPERATE");
        assert_eq!(map.overlay.unwrap()[512 * 3 + 7], 5);
        assert!(map.tiles.is_empty() && map.preview.is_none());
    }

    #[test]
    fn errors() {
        assert_eq!(
            Map::from_ini(&Ini::parse(b"[Basic]\nName=x\n")).unwrap_err(),
            Error::NoMapSection
        );
        let bad = Ini::parse(b"[Map]\nSize=0,0,1,1\n[IsoMapPack5]\n1=!!!\n");
        assert!(matches!(
            Map::from_ini(&bad),
            Err(Error::Pack {
                section: "IsoMapPack5",
                ..
            })
        ));
    }
}
