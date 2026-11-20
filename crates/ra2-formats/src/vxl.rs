//! Voxel models (`.vxl`) and their animation (`.hva`). See `docs/spec/voxel.md`.
//!
//! A VXL holds one or more sections ("limbs"): a turret, a barrel, a body.
//! Each section is a grid of columns, and each column a list of spans of
//! voxels, each voxel a palette index and a normal index. The HVA gives each
//! section a 3×4 transform per animation frame.
//!
//! The transforms and bounds are `f32` as stored. They are presentation data
//! for the renderer, never simulation state.

use std::fmt;

const MAGIC: &[u8; 16] = b"Voxel Animation\0";
const HEADER_LEN: usize = 34 + 768;
const LIMB_HEADER_LEN: usize = 28;
const TAILER_LEN: usize = 92;

/// Why a model could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    BadMagic,
    Truncated,
    /// A section's columns don't decode to its height.
    BadColumn {
        section: usize,
        column: usize,
    },
    BadHva,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::BadMagic => f.write_str("not a VXL"),
            Error::Truncated => f.write_str("VXL is truncated"),
            Error::BadColumn { section, column } => {
                write!(f, "section {section}, column {column}: bad span data")
            }
            Error::BadHva => f.write_str("not a valid HVA"),
        }
    }
}

impl std::error::Error for Error {}

/// One voxel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Voxel {
    pub x: u8,
    pub y: u8,
    pub z: u8,
    pub color: u8,
    pub normal: u8,
}

/// One section.
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub name: String,
    /// Scale applied to the HVA's translation.
    pub scale: f32,
    /// The section's own 3×4 transform, row-major.
    pub transform: [[f32; 4]; 3],
    pub min_bounds: [f32; 3],
    pub max_bounds: [f32; 3],
    pub size: [u8; 3],
    /// Which normals table the normal indices refer to: 2 for Tiberian Sun's,
    /// 4 for Red Alert 2's.
    pub normals_mode: u8,
    pub voxels: Vec<Voxel>,
}

/// A model.
#[derive(Debug, Clone, PartialEq)]
pub struct Vxl {
    /// The remap range the file declares (usually 16 to 31).
    pub remap: (u8, u8),
    pub sections: Vec<Section>,
}

fn u32_at(b: &[u8], at: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or(Error::Truncated)?
            .try_into()
            .expect("4 bytes"),
    ))
}

fn f32_at(b: &[u8], at: usize) -> Result<f32, Error> {
    Ok(f32::from_le_bytes(
        b.get(at..at + 4)
            .ok_or(Error::Truncated)?
            .try_into()
            .expect("4 bytes"),
    ))
}

fn name(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

fn to_usize(v: u32) -> Result<usize, Error> {
    usize::try_from(v).map_err(|_| Error::Truncated)
}

impl Vxl {
    pub fn parse(data: &[u8]) -> Result<Vxl, Error> {
        if data.get(..16) != Some(MAGIC.as_slice()) {
            return Err(Error::BadMagic);
        }
        let limbs = to_usize(u32_at(data, 20)?)?;
        let tailers = to_usize(u32_at(data, 24)?)?;
        let body_size = to_usize(u32_at(data, 28)?)?;
        let remap = (
            *data.get(32).ok_or(Error::Truncated)?,
            *data.get(33).ok_or(Error::Truncated)?,
        );
        if limbs != tailers || limbs == 0 || limbs > 256 {
            return Err(Error::BadMagic);
        }
        let body_start = HEADER_LEN + limbs * LIMB_HEADER_LEN;
        let tailer_start = body_start.checked_add(body_size).ok_or(Error::Truncated)?;
        if data.len() < tailer_start + limbs * TAILER_LEN {
            return Err(Error::Truncated);
        }
        let body = &data[body_start..tailer_start];
        let mut sections = Vec::with_capacity(limbs);
        for i in 0..limbs {
            let head = &data[HEADER_LEN + i * LIMB_HEADER_LEN..];
            let t = &data[tailer_start + i * TAILER_LEN..];
            let f = |k: usize| f32_at(t, 12 + 4 * k);
            let mut transform = [[0f32; 4]; 3];
            for (r, row) in transform.iter_mut().enumerate() {
                for (c, v) in row.iter_mut().enumerate() {
                    *v = f(1 + r * 4 + c)?;
                }
            }
            let size = [t[88], t[89], t[90]];
            let mut section = Section {
                name: name(&head[..16]),
                scale: f(0)?,
                transform,
                min_bounds: [f(13)?, f(14)?, f(15)?],
                max_bounds: [f(16)?, f(17)?, f(18)?],
                size,
                normals_mode: t[91],
                voxels: Vec::new(),
            };
            let starts = to_usize(u32_at(t, 0)?)?;
            let data_off = to_usize(u32_at(t, 8)?)?;
            section.voxels = decode_section(body, starts, data_off, size, i)?;
            sections.push(section);
        }
        Ok(Vxl { remap, sections })
    }
}

/// Decode one section's columns.
///
/// `starts` is the offset in the body of `x × y` `i32` span-start offsets
/// (relative to `data`, the offset of the span data; `-1` for an empty
/// column). Each column is a run of spans: a `u8` count of empty voxels to
/// skip, a `u8` count of voxels, that many (colour, normal) pairs, then the
/// count again. The spans cover the column's height exactly.
fn decode_section(
    body: &[u8],
    starts: usize,
    data: usize,
    size: [u8; 3],
    section: usize,
) -> Result<Vec<Voxel>, Error> {
    let (sx, sy, sz) = (
        usize::from(size[0]),
        usize::from(size[1]),
        usize::from(size[2]),
    );
    let mut voxels = Vec::new();
    for column in 0..sx * sy {
        let start = u32_at(body, starts + column * 4)?;
        if start == u32::MAX {
            continue;
        }
        let bad = Error::BadColumn { section, column };
        let mut at = data + to_usize(start)?;
        let mut z = 0usize;
        while z < sz {
            let skip = usize::from(*body.get(at).ok_or(bad.clone())?);
            let count = usize::from(*body.get(at + 1).ok_or(bad.clone())?);
            z += skip;
            at += 2;
            let pairs = body.get(at..at + 2 * count).ok_or(bad.clone())?;
            if z + count > sz {
                return Err(bad);
            }
            for (k, p) in pairs.as_chunks::<2>().0.iter().enumerate() {
                // Column index decomposes as x + y * sx; all fit in u8.
                voxels.push(Voxel {
                    x: u8::try_from(column % sx).expect("< 256"),
                    y: u8::try_from(column / sx).expect("< 256"),
                    z: u8::try_from(z + k).expect("< 256"),
                    color: p[0],
                    normal: p[1],
                });
            }
            z += count;
            at += 2 * count;
            if usize::from(*body.get(at).ok_or(bad.clone())?) != count {
                return Err(bad);
            }
            at += 1;
        }
        if z != sz {
            return Err(Error::BadColumn { section, column });
        }
    }
    Ok(voxels)
}

/// A voxel animation: per frame, a 3×4 transform for each section.
#[derive(Debug, Clone, PartialEq)]
pub struct Hva {
    pub name: String,
    pub section_names: Vec<String>,
    /// `frames[f][s]` is section `s`'s transform in frame `f`, row-major.
    pub frames: Vec<Vec<[[f32; 4]; 3]>>,
}

impl Hva {
    /// Parse an HVA. Its size must match its counts exactly.
    pub fn parse(data: &[u8]) -> Result<Hva, Error> {
        let frames = to_usize(u32_at(data, 16).map_err(|_| Error::BadHva)?)?;
        let sections = to_usize(u32_at(data, 20).map_err(|_| Error::BadHva)?)?;
        if frames == 0 || sections == 0 || sections > 256 || frames > 65_536 {
            return Err(Error::BadHva);
        }
        let matrices = 24 + 16 * sections;
        if data.len() != matrices + 48 * frames * sections {
            return Err(Error::BadHva);
        }
        let section_names = (0..sections)
            .map(|s| name(&data[24 + 16 * s..40 + 16 * s]))
            .collect();
        let mut out = Vec::with_capacity(frames);
        for f in 0..frames {
            let mut per = Vec::with_capacity(sections);
            for s in 0..sections {
                let base = matrices + 48 * (f * sections + s);
                let mut m = [[0f32; 4]; 3];
                for (r, row) in m.iter_mut().enumerate() {
                    for (c, v) in row.iter_mut().enumerate() {
                        *v = f32_at(data, base + 4 * (r * 4 + c))?;
                    }
                }
                per.push(m);
            }
            out.push(per);
        }
        Ok(Hva {
            name: name(&data[..16]),
            section_names,
            frames: out,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Our own encoder: one section, the given columns (each a list of
    /// (z, colour, normal)), size `sx × sy × sz`.
    fn encode(sx: u8, sy: u8, sz: u8, columns: &[Vec<(u8, u8, u8)>]) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        for v in [1u32, 1, 1] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        let mut starts = Vec::new();
        let mut ends = Vec::new();
        let mut spans = Vec::new();
        for col in columns {
            if col.is_empty() {
                starts.extend_from_slice(&(-1i32).to_le_bytes());
                ends.extend_from_slice(&(-1i32).to_le_bytes());
                continue;
            }
            starts.extend_from_slice(&u32::try_from(spans.len()).unwrap().to_le_bytes());
            let mut z = 0u8;
            for &(vz, c, n) in col {
                spans.extend_from_slice(&[vz - z, 1, c, n, 1]);
                z = vz + 1;
            }
            if z < sz {
                spans.extend_from_slice(&[sz - z, 0, 0]);
            }
            ends.extend_from_slice(&u32::try_from(spans.len() - 1).unwrap().to_le_bytes());
        }
        let body: Vec<u8> = [starts.clone(), ends, spans].concat();
        out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&[16, 31]);
        out.extend_from_slice(&[0u8; 768]);
        let mut limb = b"body".to_vec();
        limb.resize(16, 0);
        limb.extend_from_slice(&[0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&limb);
        out.extend_from_slice(&body);
        let mut tailer = Vec::new();
        tailer.extend_from_slice(&0u32.to_le_bytes());
        tailer.extend_from_slice(&u32::try_from(starts.len()).unwrap().to_le_bytes());
        tailer.extend_from_slice(&u32::try_from(starts.len() * 2).unwrap().to_le_bytes());
        for k in 0..19 {
            tailer.extend_from_slice(&(if k == 0 { 0.08f32 } else { 0.0 }).to_le_bytes());
        }
        tailer.extend_from_slice(&[sx, sy, sz, 4]);
        out.extend_from_slice(&tailer);
        out
    }

    #[test]
    fn round_trip() {
        let cols = vec![vec![(0, 5, 1), (3, 6, 2)], vec![], vec![(1, 7, 3)], vec![]];
        let v = Vxl::parse(&encode(2, 2, 4, &cols)).unwrap();
        let s = &v.sections[0];
        assert_eq!(s.name, "body");
        assert_eq!(s.size, [2, 2, 4]);
        assert_eq!(s.normals_mode, 4);
        assert_eq!(v.remap, (16, 31));
        assert_eq!(
            s.voxels,
            vec![
                Voxel {
                    x: 0,
                    y: 0,
                    z: 0,
                    color: 5,
                    normal: 1
                },
                Voxel {
                    x: 0,
                    y: 0,
                    z: 3,
                    color: 6,
                    normal: 2
                },
                Voxel {
                    x: 0,
                    y: 1,
                    z: 1,
                    color: 7,
                    normal: 3
                },
            ]
        );
    }

    #[test]
    fn damage_is_an_error_not_a_panic() {
        let base = encode(
            2,
            2,
            4,
            &[vec![(0, 5, 1), (3, 6, 2)], vec![], vec![(1, 7, 3)], vec![]],
        );
        for i in 0..base.len() {
            for v in [0u8, 1, 0x7f, 0xff] {
                let mut d = base.clone();
                d[i] = v;
                let _ = Vxl::parse(&d);
            }
        }
        assert_eq!(Vxl::parse(b"nope").unwrap_err(), Error::BadMagic);
    }

    #[test]
    fn hva() {
        let mut d = b"unit".to_vec();
        d.resize(16, 0);
        d.extend_from_slice(&2u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        let mut n = b"body".to_vec();
        n.resize(16, 0);
        d.extend_from_slice(&n);
        for f in 0u8..2 {
            for k in 0u8..12 {
                d.extend_from_slice(&(f32::from(f) * 10.0 + f32::from(k)).to_le_bytes());
            }
        }
        let h = Hva::parse(&d).unwrap();
        assert_eq!(h.section_names, ["body"]);
        assert_eq!(h.frames.len(), 2);
        assert_eq!(h.frames[1][0][2][3], 21.0);
        d.push(0);
        assert_eq!(Hva::parse(&d).unwrap_err(), Error::BadHva);
    }
}
