//! Map lighting: `[Lighting]` and lamp posts turn into a light per cell,
//! and each light into a lit palette. See `docs/spec/lighting.md`.

use ra2_formats::ini::{Ini, atoi, parse_number};
use ra2_formats::pal::Rgb;

/// A map's `[Lighting]` (normal values; the ion storm and dominator sets
/// come later), and the lamps that light cells near them.
#[derive(Debug, Clone, PartialEq)]
pub struct Lighting {
    pub ambient: f64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub ground: f64,
    /// Added to the ambient per height level.
    pub level: f64,
    pub lamps: Vec<Lamp>,
}

/// A light source: a building with `LightVisibility=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lamp {
    /// Centre, in leptons.
    pub x: i32,
    pub y: i32,
    /// `LightVisibility`, in leptons.
    pub range: i32,
    /// `LightIntensity` and the three `Light…Tint`s, in thousandths.
    pub intensity: i32,
    pub tint: [i32; 3],
}

/// One cell's light, in thousandths: intensity, then red, green and blue
/// tints. Palettes are cached by it.
pub type Light = [i32; 4];

impl Default for Lighting {
    fn default() -> Lighting {
        Lighting {
            ambient: 1.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            ground: 0.0,
            level: 0.0,
            lamps: Vec::new(),
        }
    }
}

/// A value scaled to an integer as the game reads it: × `scale`, plus 0.01,
/// truncated. `[recomp]` (0x0068A8xx)
#[allow(clippy::cast_possible_truncation)]
fn fixed(v: f64, scale: f64) -> i32 {
    (v * scale + 0.01) as i32
}

impl Lighting {
    pub fn from_map(ini: &Ini) -> Lighting {
        let d = Lighting::default();
        let get = |k: &str, default: f64| ini.get("Lighting", k).map_or(default, parse_number);
        Lighting {
            ambient: get("Ambient", d.ambient),
            red: get("Red", d.red),
            green: get("Green", d.green),
            blue: get("Blue", d.blue),
            ground: get("Ground", d.ground),
            level: get("Level", d.level),
            lamps: Vec::new(),
        }
    }

    /// Add a lamp for every building in the map's `[Structures]` whose type
    /// has `LightVisibility=` in `rules`, at its foundation's centre.
    #[must_use]
    pub fn with_lamps(mut self, map: &Ini, rules: &Ini, art: &Ini) -> Lighting {
        let Some(s) = map.section("Structures") else {
            return self;
        };
        for (_, v) in s.entries() {
            let p: Vec<&str> = v.split(',').collect();
            if p.len() < 5 {
                continue;
            }
            let id = p[1];
            let range = rules.get(id, "LightVisibility").map_or(0, atoi);
            if range <= 0 {
                continue;
            }
            let num = |k: &str| {
                rules
                    .get(id, k)
                    .map_or(0, |v| fixed(parse_number(v), 1000.0))
            };
            let image = rules.get(id, "Image").unwrap_or(id);
            let (fw, fh) = art
                .get(image, "Foundation")
                .and_then(|f| {
                    f.to_lowercase()
                        .split_once('x')
                        .map(|(a, b)| (atoi(a), atoi(b)))
                })
                .unwrap_or((1, 1));
            self.lamps.push(Lamp {
                x: atoi(p[3]) * 256 + fw * 128,
                y: atoi(p[4]) * 256 + fh * 128,
                range,
                intensity: num("LightIntensity"),
                tint: [
                    num("LightRedTint"),
                    num("LightGreenTint"),
                    num("LightBlueTint"),
                ],
            });
        }
        self
    }

    /// The light of cell (`x`, `y`) at height `level`. `[recomp]` 0x00484180:
    /// ambient and tints, plus each lamp in range scaled by
    /// (range − distance) / range, then `Level` per height level less
    /// `Ground`, the intensity capped at 2,000.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    pub fn cell(&self, x: i32, y: i32, level: i32) -> Light {
        let mut light = [
            fixed(self.ambient, 100.0) * 10,
            fixed(self.red, 100.0) * 10,
            fixed(self.green, 100.0) * 10,
            fixed(self.blue, 100.0) * 10,
        ];
        let (cx, cy) = (x * 256 + 128, y * 256 + 128);
        for l in &self.lamps {
            let (dx, dy) = (i64::from(cx - l.x), i64::from(cy - l.y));
            let d2 = dx * dx + dy * dy;
            if d2 > i64::from(l.range) * i64::from(l.range) {
                continue;
            }
            let dist = (d2 as f64).sqrt() as i32;
            if dist > l.range {
                continue;
            }
            let f = (l.range - dist) * 1000 / l.range;
            light[0] += l.intensity * f / 1000;
            for k in 0..3 {
                light[k + 1] += l.tint[k] * f / 1000;
            }
        }
        light[0] += level * fixed(self.level, 1000.0) - fixed(self.ground, 1000.0);
        light[0] = light[0].min(2000);
        light
    }

    /// A palette lit for a height level with no lamps.
    pub fn apply(&self, palette: &[Rgb; 256], height: i32) -> [Rgb; 256] {
        let mut no_lamps = self.clone();
        no_lamps.lamps.clear();
        Lighting::lit(palette, no_lamps.cell(0, 0, height))
    }

    /// A palette lit by a cell's light: each channel times intensity times
    /// its tint, clamped to 0–2. `[hypothesis]`: the shape is from
    /// `LightConvertClass` (`0x00555DA0`); the exact rounding isn't traced.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn lit(palette: &[Rgb; 256], light: Light) -> [Rgb; 256] {
        let intensity = f64::from(light[0]) / 1000.0;
        let tint = [1, 2, 3].map(|k| (f64::from(light[k]) / 1000.0 * intensity).clamp(0.0, 2.0));
        let mut out = *palette;
        for c in out.iter_mut().skip(1) {
            for k in 0..3 {
                c[k] = (f64::from(c[k]) * tint[k]).round().clamp(0.0, 255.0) as u8;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lighting_tints_and_brightens_by_level() {
        let ini =
            Ini::parse(b"[Lighting]\nAmbient=0.75\nRed=1.0\nGreen=1.0\nBlue=1.5\nLevel=0.1\n");
        let l = Lighting::from_map(&ini);
        let mut pal = [[100u8; 3]; 256];
        pal[0] = [0, 0, 0];
        let lit = l.apply(&pal, 0);
        assert_eq!(lit[5], [75, 75, 113]);
        assert_eq!(l.apply(&pal, 5)[5], [125, 125, 188]);
        assert_eq!(lit[0], [0, 0, 0], "index 0 is transparent and left alone");
        assert_eq!(Lighting::default().apply(&pal, 3)[9], [100, 100, 100]);
    }

    #[test]
    fn ground_is_taken_off_and_lamps_add_light_falling_off_with_distance() {
        let l = Lighting {
            ground: 0.05,
            ..Lighting::default()
        };
        assert_eq!(l.cell(0, 0, 0), [950, 1000, 1000, 1000]);
        let l = Lighting {
            lamps: vec![Lamp {
                x: 128,
                y: 128,
                range: 1024,
                intensity: 200,
                tint: [50, 50, 10],
            }],
            ..Lighting::default()
        };
        assert_eq!(l.cell(0, 0, 0), [1200, 1050, 1050, 1010]);
        assert_eq!(l.cell(2, 0, 0)[0], 1100, "half way: half the light");
        assert_eq!(l.cell(5, 0, 0)[0], 1000, "out of range");
    }
}
