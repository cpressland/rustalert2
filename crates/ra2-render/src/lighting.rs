//! Map lighting: `[Lighting]` turns into palettes lit per height level.
//! See `docs/spec/lighting.md`.

use ra2_formats::ini::{Ini, parse_number};
use ra2_formats::pal::Rgb;

/// A map's `[Lighting]` (normal values; the ion storm and dominator sets
/// come later).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    pub ambient: f64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub ground: f64,
    /// Added to the ambient per height level.
    pub level: f64,
}

impl Default for Lighting {
    fn default() -> Lighting {
        Lighting {
            ambient: 1.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
            ground: 0.0,
            level: 0.0,
        }
    }
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
        }
    }

    /// A palette lit for a height level: each channel times
    /// `(ambient + level × height)` times its tint, clamped. `[hypothesis]`:
    /// the shape is from `LightConvertClass` (`0x00555DA0`: tints in
    /// thousandths, clamped at 2,000); the exact rounding isn't traced.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn apply(&self, palette: &[Rgb; 256], height: i32) -> [Rgb; 256] {
        let intensity = self.ambient + self.level * f64::from(height);
        let tint = [self.red, self.green, self.blue].map(|t| (t * intensity).clamp(0.0, 2.0));
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
}
