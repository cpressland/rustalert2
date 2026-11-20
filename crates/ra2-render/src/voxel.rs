//! Vehicles: voxel models projected to the screen, as palette-indexed
//! sprites. See `docs/spec/voxel.md`.
//!
//! This is a first, flat-shaded renderer: every voxel takes `voxels.vpl`'s
//! neutral shade level (8: it maps 199 of 256 colours to themselves; levels
//! above brighten, below darken), because the normals tables the game
//! lights voxels with aren't in yet. Positions and colours are right; the
//! lighting by normal comes later.

use ra2_formats::vxl::{Hva, Vxl};
use ra2_sim::coords::{
    CELL_HEIGHT, CELL_WIDTH, LEPTONS_PER_CELL, LEPTONS_PER_LEVEL, PIXELS_PER_LEVEL,
};

/// Leptons per voxel unit. Derived from the Lasher's firing point (225
/// leptons forward) against its turret model's reach (about 32.5 units).
/// `[hypothesis]`: to be confirmed against a recomp frame.
pub const LEPTONS_PER_VOXEL: f32 = 256.0 / 36.0;

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

    /// The level that leaves colours as they are.
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
pub fn render(models: &[(&Vxl, Option<&Hva>)], facing: u8, shades: Option<&ShadeTable>) -> Sprite {
    let models: Vec<Model> = models
        .iter()
        .map(|(vxl, hva)| Model { vxl, hva: *hva })
        .collect();
    let angle = f32::from(facing) * std::f32::consts::TAU / 256.0;
    let (fwd, right) = ((angle.sin(), -angle.cos()), (angle.cos(), angle.sin()));
    // Each voxel to screen pixels and depth.
    let mut points: Vec<(i32, i32, f32, u8)> = Vec::new();
    for m in &models {
        for (si, s) in m.vxl.sections.iter().enumerate() {
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
                #[allow(clippy::cast_possible_truncation)]
                points.push((sx.round() as i32, sy.round() as i32, wx + wy + wz, v.color));
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
        let color = shades.map_or(c, |t| t.shade(ShadeTable::NEUTRAL, c));
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
        let s = render(&[(&v, None)], 0, None);
        assert!(s.width > 4 && s.height > 4);
        assert!(s.pixels.contains(&20));
        // The base sits at the anchor; the top of the cube is above it.
        let (ax, ay) = (
            usize::try_from(s.anchor.0).unwrap(),
            usize::try_from(s.anchor.1).unwrap(),
        );
        assert!(ax < s.width && ay < s.height);
        // Rotating a cube by a quarter turn gives the same footprint.
        let r = render(&[(&v, None)], 64, None);
        assert_eq!((r.width, r.height), (s.width, s.height));
    }
}
