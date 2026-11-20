//! Isometric terrain tiles (`.tem`, `.sno`, `.urb`, `.des`, `.lun`, `.ubn`)
//! and the theater INIs that group them into tile sets.
//! See `docs/spec/tmp.md`.

use std::fmt;

use crate::ini::Ini;

const FILE_HEADER_LEN: usize = 16;
const CELL_HEADER_LEN: usize = 52;

/// Why a tile file could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    NotTmp(&'static str),
    Truncated { cell: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotTmp(why) => write!(f, "not a TS/RA2 TMP: {why}"),
            Error::Truncated { cell } => write!(f, "cell {cell} runs past the end of the file"),
        }
    }
}

impl std::error::Error for Error {}

/// One cell of a tile template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// Where the cell's diamond sits, in pixels, relative to the template.
    pub x: i32,
    pub y: i32,
    /// The height level of the cell.
    pub height: u8,
    pub terrain_type: u8,
    pub ramp_type: u8,
    pub radar_left: [u8; 3],
    pub radar_right: [u8; 3],
    /// The diamond's palette indices, row by row (see [`diamond_rows`]).
    pub image: Vec<u8>,
    /// Per-pixel depth for the diamond, if the cell has it.
    pub z: Option<Vec<u8>>,
    /// Art that sticks up out of the diamond (cliffs, buildings on tiles).
    pub extra: Option<Extra>,
}

/// Extra art drawn over and beyond a cell's diamond.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extra {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub image: Vec<u8>,
    pub z: Option<Vec<u8>>,
}

/// A tile template: a grid of cells, some of them empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tmp {
    /// The template's size in cells.
    pub width: u32,
    pub height: u32,
    /// The size of one cell's diamond in pixels (60×30 in RA2).
    pub tile_width: u32,
    pub tile_height: u32,
    /// Row-major, `width × height`; `None` where the template has no cell.
    pub cells: Vec<Option<Cell>>,
}

/// The rows of a diamond: for each row, its first column and width. A
/// diamond `w × h` holds `w × h / 2` pixels: rows widen by 4 to the middle,
/// then narrow by 4, the last row empty.
pub fn diamond_rows(tile_width: u32, tile_height: u32) -> Vec<(u32, u32)> {
    let half = tile_height / 2;
    (0..tile_height)
        .map(|y| {
            let w = if y < half {
                4 * (y + 1)
            } else {
                tile_width.saturating_sub(4 * (y + 1 - half))
            };
            ((tile_width - w.min(tile_width)) / 2, w.min(tile_width))
        })
        .collect()
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn us(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

impl Tmp {
    pub fn parse(data: &[u8]) -> Result<Tmp, Error> {
        let header = |at| u32_at(data, at).ok_or(Error::NotTmp("shorter than a header"));
        let (width, height, tile_width, tile_height) =
            (header(0)?, header(4)?, header(8)?, header(12)?);
        if width == 0 || height == 0 || width > 64 || height > 64 {
            return Err(Error::NotTmp("implausible template size"));
        }
        if tile_width == 0
            || tile_height == 0
            || tile_width > 256
            || tile_height > 128
            || tile_width != 2 * tile_height
        {
            return Err(Error::NotTmp("implausible tile size"));
        }
        let count = us(width * height);
        let image_len = us(tile_width * tile_height / 2);
        let mut cells = Vec::with_capacity(count);
        for i in 0..count {
            let off = us(u32_at(data, FILE_HEADER_LEN + 4 * i)
                .ok_or(Error::NotTmp("offset table is truncated"))?);
            if off == 0 {
                cells.push(None);
                continue;
            }
            let bad = Error::Truncated { cell: i };
            let h = data.get(off..off + CELL_HEADER_LEN).ok_or(bad.clone())?;
            let flags = u32_at(h, 36).expect("in header");
            let image = data
                .get(off + CELL_HEADER_LEN..off + CELL_HEADER_LEN + image_len)
                .ok_or(bad.clone())?
                .to_vec();
            let z = if flags & 2 != 0 {
                let at = off + us(u32_at(h, 12).expect("in header"));
                Some(data.get(at..at + image_len).ok_or(bad.clone())?.to_vec())
            } else {
                None
            };
            let extra = if flags & 1 != 0 {
                let (ew, eh) = (
                    u32_at(h, 28).expect("in header"),
                    u32_at(h, 32).expect("in header"),
                );
                let len = us(ew).checked_mul(us(eh)).ok_or(bad.clone())?;
                let at = off + us(u32_at(h, 8).expect("in header"));
                let image = data.get(at..at + len).ok_or(bad.clone())?.to_vec();
                let z = if flags & 2 != 0 {
                    let at = off + us(u32_at(h, 16).expect("in header"));
                    Some(data.get(at..at + len).ok_or(bad.clone())?.to_vec())
                } else {
                    None
                };
                Some(Extra {
                    x: i32_at(h, 20).expect("in header"),
                    y: i32_at(h, 24).expect("in header"),
                    width: ew,
                    height: eh,
                    image,
                    z,
                })
            } else {
                None
            };
            cells.push(Some(Cell {
                x: i32_at(h, 0).expect("in header"),
                y: i32_at(h, 4).expect("in header"),
                height: h[40],
                terrain_type: h[41],
                ramp_type: h[42],
                radar_left: [h[43], h[44], h[45]],
                radar_right: [h[46], h[47], h[48]],
                image,
                z,
                extra,
            }));
        }
        Ok(Tmp {
            width,
            height,
            tile_width,
            tile_height,
            cells,
        })
    }
}

/// One tile set from a theater INI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileSet {
    /// The `[TileSetNNNN]` number.
    pub number: u32,
    pub name: String,
    /// The base of its files' names: tile *n* is `{file_name}{n:02}.{ext}`.
    pub file_name: String,
    pub tiles: u32,
    /// For a "Marble Madness" set, the ordinary set it previews in the map
    /// editor. Such sets have no tiles in the game's archives.
    pub non_marble_madness: Option<u32>,
    /// `AllowToPlace=no` marks sets the editor may not place by hand. The
    /// game still uses them: its automatic transitions place them, and maps
    /// are full of them.
    pub allow_to_place: bool,
}

impl TileSet {
    /// A "Marble Madness" set: the editor's view of another set, named by
    /// `NonMarbleMadness`. Its files are usually absent from the game's
    /// archives even though maps use its tile numbers.
    pub fn is_marble_madness(&self) -> bool {
        self.non_marble_madness.is_some()
    }
}

/// The tile sets of a theater INI, in order. A map's tile number counts
/// through them: set 0's tiles first, then set 1's, and so on.
pub fn tile_sets(theater_ini: &Ini) -> Vec<TileSet> {
    let mut out = Vec::new();
    for number in 0.. {
        let section = format!("TileSet{number:04}");
        let Some(s) = theater_ini.section(&section) else {
            break;
        };
        let tiles = s.get("TilesInSet").map_or(0, crate::ini::atoi);
        out.push(TileSet {
            number,
            name: s.get("SetName").unwrap_or_default().to_string(),
            file_name: s.get("FileName").unwrap_or_default().to_string(),
            tiles: u32::try_from(tiles).unwrap_or(0),
            non_marble_madness: s
                .get("NonMarbleMadness")
                .and_then(|v| u32::try_from(crate::ini::atoi(v)).ok()),
            allow_to_place: s
                .get("AllowToPlace")
                .and_then(crate::ini::parse_bool)
                .unwrap_or(true),
        });
    }
    out
}

/// The file name of a map tile number, given the theater's tile sets and
/// file extension; `None` past the last set.
pub fn tile_file(sets: &[TileSet], mut tile: u32, ext: &str) -> Option<String> {
    for set in sets {
        if tile < set.tiles {
            return Some(format!("{}{:02}.{ext}", set.file_name, tile + 1));
        }
        tile -= set.tiles;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diamond_holds_half_its_box() {
        let rows = diamond_rows(60, 30);
        assert_eq!(rows.iter().map(|r| r.1).sum::<u32>(), 900);
        assert_eq!(rows[0], (28, 4));
        assert_eq!(rows[14], (0, 60));
        assert_eq!(rows[15], (2, 56));
        assert_eq!(rows[29], (30, 0));
    }

    fn encode(cells: &[Option<(u8, bool)>]) -> Vec<u8> {
        let mut out = Vec::new();
        let n = u32::try_from(cells.len()).unwrap();
        for v in [n, 1, 60, 30] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let mut body = Vec::new();
        let table_end = 16 + 4 * cells.len();
        let mut offsets = Vec::new();
        for c in cells {
            match c {
                None => offsets.push(0u32),
                Some((fill, extra)) => {
                    offsets.push(u32::try_from(table_end + body.len()).unwrap());
                    let mut h = vec![0u8; 52];
                    let extra_at = 52 + 900;
                    if *extra {
                        h[8..12].copy_from_slice(&u32::try_from(extra_at).unwrap().to_le_bytes());
                        h[28..32].copy_from_slice(&3u32.to_le_bytes());
                        h[32..36].copy_from_slice(&2u32.to_le_bytes());
                        h[36] = 1;
                    }
                    h[40] = 4;
                    body.extend_from_slice(&h);
                    body.extend(std::iter::repeat_n(*fill, 900));
                    if *extra {
                        body.extend_from_slice(&[9; 6]);
                    }
                }
            }
        }
        for o in offsets {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn cells_and_extra() {
        let t = Tmp::parse(&encode(&[Some((7, false)), None, Some((8, true))])).unwrap();
        assert_eq!(
            (t.width, t.height, t.tile_width, t.tile_height),
            (3, 1, 60, 30)
        );
        let a = t.cells[0].as_ref().unwrap();
        assert_eq!(a.image.len(), 900);
        assert_eq!(a.height, 4);
        assert!(a.extra.is_none() && a.z.is_none());
        assert!(t.cells[1].is_none());
        let e = t.cells[2].as_ref().unwrap().extra.as_ref().unwrap();
        assert_eq!((e.width, e.height, e.image.clone()), (3, 2, vec![9; 6]));
        // Found by fuzzing: a huge tile height overflowed `2 * tile_height`.
        let mut huge = encode(&[None]);
        huge[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Tmp::parse(&huge).is_err());
        let mut short = encode(&[Some((7, true))]);
        short.truncate(short.len() - 1);
        assert!(matches!(
            Tmp::parse(&short),
            Err(Error::Truncated { cell: 0 })
        ));
    }

    #[test]
    fn tile_numbers() {
        let ini = Ini::parse(
            b"[TileSet0000]\nSetName=Clear\nFileName=clear\nTilesInSet=1\n\
              [TileSet0001]\nSetName=Water\nFileName=water\nTilesInSet=3\n\
              [TileSet0002]\nSetName=ZMM Water\nFileName=mwater\nTilesInSet=2\nNonMarbleMadness=1\n",
        );
        let sets = tile_sets(&ini);
        assert_eq!(sets.len(), 3);
        assert!(!sets[1].is_marble_madness());
        assert!(sets[2].is_marble_madness());
        assert_eq!(tile_file(&sets, 0, "tem").as_deref(), Some("clear01.tem"));
        assert_eq!(tile_file(&sets, 3, "tem").as_deref(), Some("water03.tem"));
        assert_eq!(tile_file(&sets, 4, "tem").as_deref(), Some("mwater01.tem"));
        assert_eq!(tile_file(&sets, 6, "tem"), None);
    }
}
