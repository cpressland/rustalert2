//! Vehicles: voxel models projected to the screen, as palette-indexed
//! sprites. See `docs/spec/voxel.md`.
//!
//! Each voxel is lit by its normal: the game builds a 256-entry table of
//! shade levels for the unit's facing (diffuse plus a specular term) and
//! looks each voxel's colour up in `voxels.vpl` at its normal's level. The
//! normals themselves are read from the player's `gamemd.exe`; without it,
//! voxels are flat-shaded at the neutral level.

use ra2_formats::vxl::{Hva, Vxl};
use ra2_sim::coords::{
    CELL_HEIGHT, CELL_WIDTH, LEPTONS_PER_CELL, LEPTONS_PER_LEVEL, PIXELS_PER_LEVEL,
};

/// Leptons per voxel unit. Derived from the Lasher's firing point (225
/// leptons forward) against its turret model's reach (about 32.5 units).
/// `[hypothesis]`: to be confirmed against a recomp frame.
pub const LEPTONS_PER_VOXEL: f32 = 256.0 / 36.0;

/// The light, in the world frame (map x and y, z up), as the game sets it:
/// `(-1/sqrt 2, -1/sqrt 2, 0)` turned a quarter of pi about y.
/// `[recomp 0x00754C00]`
pub const LIGHT: [f32; 3] = [-0.5, -std::f32::consts::FRAC_1_SQRT_2, 0.5];

/// The specular term's sharpness. `[recomp 0x00706F23]`
const SPECULAR_K: f64 = 3.0;

/// Shade levels per unit of light. `[recomp 0x007F6960]`
const SHADE_SCALE: f64 = 16.0;

/// The voxel normal sets, by a section's normals mode (1–4).
#[derive(Debug, Clone, Default)]
pub struct Normals {
    sets: Vec<Vec<[f32; 3]>>,
}

impl Normals {
    /// Where `gamemd.exe` 1.001 keeps the pointers to its normal sets, and
    /// their counts, one per normals mode. `[recomp 0x00758670]`
    const POINTERS: u32 = 0x0084_69E0;
    const COUNTS: u32 = 0x0084_69F4;

    /// Read the normal sets out of `gamemd.exe` 1.001. `None` for any other
    /// file, or if a table doesn't look like unit vectors.
    pub fn from_gamemd(exe: &[u8]) -> Option<Normals> {
        let pe = ra2_formats::pe::Pe::parse(exe)?;
        let mut sets = Vec::with_capacity(5);
        for mode in 0..5u32 {
            let ptr = pe.u32(Self::POINTERS + mode * 4)?;
            let count = pe.u32(Self::COUNTS + mode * 4)?;
            if ptr == 0 || count == 0 {
                sets.push(Vec::new());
                continue;
            }
            if count > 256 {
                return None;
            }
            let mut set = Vec::with_capacity(count as usize);
            for i in 0..count {
                let n = [0, 1, 2].map(|k| pe.f32(ptr + i * 12 + k * 4));
                let [Some(x), Some(y), Some(z)] = n else {
                    return None;
                };
                if !(0.9..=1.1).contains(&(x * x + y * y + z * z)) {
                    return None;
                }
                set.push([x, y, z]);
            }
            sets.push(set);
        }
        Some(Normals { sets })
    }

    /// How many normals mode `mode` has.
    pub fn count(&self, mode: u8) -> usize {
        self.sets.get(usize::from(mode)).map_or(0, Vec::len)
    }

    /// The shade level of each normal index for a section of normals mode
    /// `mode`, lit by `light` (in the model's frame) and viewed from the
    /// model's up axis. `[recomp 0x007586F0]`
    ///
    /// For each normal `n`: diffuse `d = max(n·L, 0)`; with `H` the
    /// normalised `L + V`, `s = n·H` and specular `s / (k − k·s + s)`
    /// (Schlick's approximation, `k = 3`), dropped below 0. The level is
    /// `16 × (d + specular)`, truncated. Indices past the set are 0, but the
    /// last three are always 16.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn shade_table(&self, mode: u8, light: [f32; 3]) -> [u8; 256] {
        let mut table = [0u8; 256];
        let l = light.map(f64::from);
        let v = [0.0, 0.0, 1.0];
        let h = [l[0] + v[0], l[1] + v[1], l[2] + v[2]];
        let len = (h[0] * h[0] + h[1] * h[1] + h[2] * h[2]).sqrt();
        let h = if len == 0.0 { h } else { h.map(|c| c / len) };
        let dot = |n: &[f32; 3], a: [f64; 3]| {
            f64::from(n[0]) * a[0] + f64::from(n[1]) * a[1] + f64::from(n[2]) * a[2]
        };
        for (i, n) in self
            .sets
            .get(usize::from(mode))
            .into_iter()
            .flatten()
            .enumerate()
            .take(256)
        {
            let s = dot(n, h);
            let spec = s / (SPECULAR_K - SPECULAR_K * s + s);
            let spec = if spec < 0.0 { 0.0 } else { spec };
            let level = dot(n, l).max(0.0) + spec;
            table[i] = if level < 0.0 {
                0
            } else {
                (level * SHADE_SCALE) as u8
            };
        }
        table[253..].fill(16);
        table
    }
}

/// `voxels.vpl`: a shade table, palette index × 32 shade levels.
#[derive(Debug, Clone)]
pub struct ShadeTable {
    sections: Vec<[u8; 256]>,
}

impl ShadeTable {
    /// Parse `voxels.vpl`: a 16-byte header (remap start, end, section
    /// count, unused), a 768-byte palette, then the sections.
    pub fn parse(data: &[u8]) -> Option<ShadeTable> {
        let count = usize::try_from(u32::from_le_bytes(data.get(8..12)?.try_into().ok()?)).ok()?;
        let body = data.get(16 + 768..16 + 768 + count * 256)?;
        Some(ShadeTable {
            sections: body.as_chunks::<256>().0.to_vec(),
        })
    }

    /// The level that leaves colours as they are, for unlit voxels.
    pub const NEUTRAL: usize = 8;

    fn shade(&self, level: usize, index: u8) -> u8 {
        self.sections
            .get(level.min(self.sections.len().saturating_sub(1)))
            .map_or(index, |s| s[usize::from(index)])
    }
}

/// A rendered vehicle: palette indices, and where its centre falls.
#[derive(Debug, Clone, Default)]
pub struct Sprite {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
    /// The unit's ground position within the sprite.
    pub anchor: (i32, i32),
}

struct Model<'a> {
    vxl: &'a Vxl,
    hva: Option<&'a Hva>,
}

/// Project points of several models with one facing into a sprite.
///
/// With `normals`, each voxel is lit by its normal for this facing (the
/// light is turned into the model's frame; sections' own HVA rotations
/// don't turn it further, as in the game); without, it's flat-shaded.
pub fn render(
    models: &[(&Vxl, Option<&Hva>)],
    facing: u8,
    shades: Option<&ShadeTable>,
    normals: Option<&Normals>,
) -> Sprite {
    let models: Vec<Model> = models
        .iter()
        .map(|(vxl, hva)| Model { vxl, hva: *hva })
        .collect();
    let angle = f32::from(facing) * std::f32::consts::TAU / 256.0;
    let (fwd, right) = ((angle.sin(), -angle.cos()), (angle.cos(), angle.sin()));
    // The light in the model's frame: x forward, y to the left, z up.
    let light = [
        LIGHT[0] * fwd.0 + LIGHT[1] * fwd.1,
        -(LIGHT[0] * right.0 + LIGHT[1] * right.1),
        LIGHT[2],
    ];
    let mut tables: [Option<[u8; 256]>; 5] = [None; 5];
    // Each voxel to screen pixels, depth and colour.
    let mut points: Vec<(i32, i32, f32, u8)> = Vec::new();
    for m in &models {
        for (si, s) in m.vxl.sections.iter().enumerate() {
            let mode = usize::from(s.normals_mode).min(4);
            let table = normals.map(|n| {
                *tables[mode].get_or_insert_with(|| n.shade_table(s.normals_mode.min(4), light))
            });
            let size = s.size.map(|v| f32::from(v.max(1)));
            let step = [0, 1, 2].map(|k| (s.max_bounds[k] - s.min_bounds[k]) / size[k]);
            let t = m
                .hva
                .and_then(|h| h.frames.first())
                .and_then(|f| f.get(si))
                .copied();
            for v in &s.voxels {
                let local = [
                    s.min_bounds[0] + (f32::from(v.x) + 0.5) * step[0],
                    s.min_bounds[1] + (f32::from(v.y) + 0.5) * step[1],
                    s.min_bounds[2] + (f32::from(v.z) + 0.5) * step[2],
                ];
                let p = match t {
                    Some(mat) => [0, 1, 2].map(|r| {
                        mat[r][0] * local[0]
                            + mat[r][1] * local[1]
                            + mat[r][2] * local[2]
                            + mat[r][3] * s.scale
                    }),
                    None => local,
                };
                // Model x forward, y to the left, z up.
                let wx = (p[0] * fwd.0 - p[1] * right.0) * LEPTONS_PER_VOXEL;
                let wy = (p[0] * fwd.1 - p[1] * right.1) * LEPTONS_PER_VOXEL;
                let wz = p[2] * LEPTONS_PER_VOXEL;
                let half_w = f32::from(i16::try_from(CELL_WIDTH / 2).expect("small"));
                let half_h = f32::from(i16::try_from(CELL_HEIGHT / 2).expect("small"));
                let lpc = f32::from(i16::try_from(LEPTONS_PER_CELL).expect("small"));
                let ppl = f32::from(i16::try_from(PIXELS_PER_LEVEL).expect("small"));
                let lpl = f32::from(i16::try_from(LEPTONS_PER_LEVEL).expect("small"));
                let sx = (wx - wy) * half_w / lpc;
                let sy = (wx + wy) * half_h / lpc - wz * ppl / lpl;
                let color = match (shades, table) {
                    (Some(t), Some(levels)) => {
                        t.shade(usize::from(levels[usize::from(v.normal)]), v.color)
                    }
                    (Some(t), None) => t.shade(ShadeTable::NEUTRAL, v.color),
                    (None, _) => v.color,
                };
                #[allow(clippy::cast_possible_truncation)]
                points.push((sx.round() as i32, sy.round() as i32, wx + wy + wz, color));
            }
        }
    }
    if points.is_empty() {
        return Sprite::default();
    }
    let (x0, y0) = (
        points.iter().map(|p| p.0).min().unwrap_or(0) - 1,
        points.iter().map(|p| p.1).min().unwrap_or(0) - 1,
    );
    let (x1, y1) = (
        points.iter().map(|p| p.0).max().unwrap_or(0) + 2,
        points.iter().map(|p| p.1).max().unwrap_or(0) + 2,
    );
    let (w, h) = (
        usize::try_from(x1 - x0).unwrap_or(1),
        usize::try_from(y1 - y0).unwrap_or(1),
    );
    let mut pixels = vec![0u8; w * h];
    let mut depth = vec![f32::MIN; w * h];
    for (x, y, d, c) in points {
        let color = c;
        // Each voxel is plotted two pixels wide, as the game's rasterizers do.
        for dx in 0..2 {
            let (px, py) = (
                usize::try_from(x - x0 + dx).unwrap_or(0),
                usize::try_from(y - y0).unwrap_or(0),
            );
            if px < w && py < h && d > depth[py * w + px] {
                depth[py * w + px] = d;
                pixels[py * w + px] = color.max(1);
            }
        }
    }
    Sprite {
        width: w,
        height: h,
        pixels,
        anchor: (-x0, -y0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2_formats::vxl::{Section, Voxel};

    fn cube() -> Vxl {
        let voxels = (0..4u8)
            .flat_map(|x| {
                (0..4u8).flat_map(move |y| {
                    (0..4u8).map(move |z| Voxel {
                        x,
                        y,
                        z,
                        color: 20,
                        normal: 0,
                    })
                })
            })
            .collect();
        Vxl {
            remap: (16, 31),
            sections: vec![Section {
                name: "body".into(),
                scale: 1.0 / 12.0,
                transform: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                ],
                min_bounds: [-2.0, -2.0, 0.0],
                max_bounds: [2.0, 2.0, 4.0],
                size: [4, 4, 4],
                normals_mode: 4,
                voxels,
            }],
        }
    }

    #[test]
    fn cube_projects_around_its_anchor() {
        let v = cube();
        let s = render(&[(&v, None)], 0, None, None);
        assert!(s.width > 4 && s.height > 4);
        assert!(s.pixels.contains(&20));
        // The base sits at the anchor; the top of the cube is above it.
        let (ax, ay) = (
            usize::try_from(s.anchor.0).unwrap(),
            usize::try_from(s.anchor.1).unwrap(),
        );
        assert!(ax < s.width && ay < s.height);
        // Rotating a cube by a quarter turn gives the same footprint.
        let r = render(&[(&v, None)], 64, None, None);
        assert_eq!((r.width, r.height), (s.width, s.height));
    }

    #[test]
    fn shade_table_lights_faces_towards_the_light() {
        let n = Normals {
            sets: vec![
                vec![],
                vec![
                    [0.0, 0.0, 1.0],
                    [0.0, 0.0, -1.0],
                    LIGHT,
                    [0.5, 0.707_106_77, -0.5],
                ],
            ],
        };
        let t = n.shade_table(1, LIGHT);
        // Straight up: diffuse 0.5, plus specular from the half vector.
        assert!(t[0] > 8 && t[0] < 32, "up {}", t[0]);
        assert_eq!(t[1], 0, "facing down gets no light");
        assert!(t[2] >= 16, "facing the light gets the full diffuse");
        assert_eq!(t[3], 0, "facing away gets nothing");
        assert_eq!(t[4], 0, "past the set");
        assert_eq!(&t[253..], &[16, 16, 16]);
    }
}
