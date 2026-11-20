//! Palettes (`.pal`) and house colours. See `docs/spec/palette.md`.

use std::f64::consts::FRAC_PI_2;

/// The palette slots a house colour replaces.
pub const REMAP_START: usize = 16;
pub const REMAP_LEN: usize = 16;

/// An 8-bit RGB colour.
pub type Rgb = [u8; 3];

/// A 256-colour palette as stored: 6-bit components, 0 to 63.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Palette {
    pub colors: [[u8; 3]; 256],
}

impl Palette {
    /// Parse a 768-byte `.pal`. Values above 63 are masked to 6 bits, as a
    /// VGA DAC would.
    pub fn parse(data: &[u8]) -> Option<Palette> {
        if data.len() != 768 {
            return None;
        }
        let mut colors = [[0u8; 3]; 256];
        for (c, raw) in colors.iter_mut().zip(data.as_chunks::<3>().0) {
            *c = raw.map(|v| v & 0x3f);
        }
        Some(Palette { colors })
    }

    /// The palette in 8-bit components: each 6-bit value shifted left by 2.
    pub fn to_rgb8(&self) -> [Rgb; 256] {
        self.colors.map(|c| c.map(|v| v << 2))
    }
}

/// A colour in the game's HSV: hue, saturation and value, each 0 to 255.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hsv {
    pub h: u8,
    pub s: u8,
    pub v: u8,
}

impl Hsv {
    /// Parse a `[Colors]` value, `H,S,V`. Missing or bad parts are 0, and
    /// each part keeps its low 8 bits, as `sscanf("%d,%d,%d")` into bytes would.
    pub fn parse(value: &str) -> Hsv {
        let mut parts = value
            .split(',')
            .map(|p| crate::ini::atoi(p).to_le_bytes()[0]);
        Hsv {
            h: parts.next().unwrap_or(0),
            s: parts.next().unwrap_or(0),
            v: parts.next().unwrap_or(0),
        }
    }

    /// Convert to RGB with the game's integer arithmetic (`[recomp 0x00517440]`).
    pub fn to_rgb(self) -> Rgb {
        let (h, s, v) = (u32::from(self.h), u32::from(self.s), u32::from(self.v));
        let six_h = 6 * h;
        let sector = six_h / 255;
        let f = six_h % 255;
        let p = v * (255 - s) / 255;
        let q = v * (255 - f * s / 255) / 255;
        let t = v * (255 - (255 - f) * s / 255) / 255;
        let (r, g, b) = match sector {
            0 | 6 => (v, t, p),
            1 => (q, v, p),
            2 => (p, v, t),
            3 => (p, q, v),
            4 => (t, p, v),
            _ => (v, p, q),
        };
        // Every term is at most 255.
        [r, g, b].map(|c| u8::try_from(c).expect("<= 255"))
    }
}

/// The game's fast sine, from its 8,192-step table (`[recomp 0x004CACB0]`):
/// the angle is quantised to the nearest table step.
#[allow(
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
)]
fn table_sin(x: f64, offset: i64) -> f64 {
    // x * 2607.5945 is twice the table index; halve it, rounding up the half.
    let doubled = (x * f64::from(2_607.594_5_f32)) as i64;
    let index = (doubled / 2 + (doubled & 1) + offset).rem_euclid(8192);
    // The table holds `f32`s.
    f64::from((index as f64 * std::f64::consts::TAU / 8192.0).sin() as f32)
}

/// The sixteen colours a house's `[Colors]` entry turns into, for palette
/// slots 16 to 31 (`[recomp 0x0068C3B0]`).
///
/// For step `i`, the hue is kept, the saturation is scaled by
/// `sin(50° + i·2.667°)` and the value by `cos(20° + i·4.667°)`. Step 0 uses
/// an angle of 90° for the value, so slot 16 comes out black.
#[allow(
    clippy::float_arithmetic,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
pub fn house_ramp(base: Hsv) -> [Rgb; REMAP_LEN] {
    let mut out = [[0u8; 3]; REMAP_LEN];
    for (i, slot) in out.iter_mut().enumerate() {
        let step = f64::from(u8::try_from(i).expect("< 16"));
        let value_angle = if i == 0 {
            FRAC_PI_2
        } else {
            step * 0.081_448_698_426_402_04 + 0.349_065_850_398_865_9
        };
        let sat_angle = step * 0.046_542_113_386_515_455 + 0.872_664_625_997_164_8;
        // Truncated toward zero, as the original's `_ftol` does.
        let s = (table_sin(sat_angle, 0) * f64::from(base.s)) as u8;
        let v = (table_sin(value_angle, 2048) * f64::from(base.v)) as u8;
        *slot = Hsv { h: base.h, s, v }.to_rgb();
    }
    out
}

/// A palette with a house's colours in the remap slots.
pub fn remap(palette: &[Rgb; 256], base: Hsv) -> [Rgb; 256] {
    let mut out = *palette;
    out[REMAP_START..REMAP_START + REMAP_LEN].copy_from_slice(&house_ramp(base));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_scale() {
        let mut data = vec![0u8; 768];
        data[3..6].copy_from_slice(&[63, 32, 0xff]);
        let p = Palette::parse(&data).unwrap();
        assert_eq!(p.colors[1], [63, 32, 63]);
        assert_eq!(p.to_rgb8()[1], [252, 128, 252]);
        assert!(Palette::parse(&data[..767]).is_none());
    }

    #[test]
    fn hsv_to_rgb() {
        assert_eq!(
            Hsv {
                h: 0,
                s: 255,
                v: 255
            }
            .to_rgb(),
            [255, 0, 0]
        );
        assert_eq!(
            Hsv {
                h: 85,
                s: 255,
                v: 255
            }
            .to_rgb(),
            [0, 255, 0]
        );
        assert_eq!(
            Hsv {
                h: 170,
                s: 255,
                v: 255
            }
            .to_rgb(),
            [0, 0, 255]
        );
        assert_eq!(Hsv { h: 0, s: 0, v: 200 }.to_rgb(), [200, 200, 200]);
        assert_eq!(
            Hsv {
                h: 255,
                s: 255,
                v: 255
            }
            .to_rgb(),
            [255, 0, 0],
            "hue 255 wraps to red"
        );
        assert_eq!(
            Hsv::parse("0,220,180"),
            Hsv {
                h: 0,
                s: 220,
                v: 180
            }
        );
        assert_eq!(Hsv::parse("1,2"), Hsv { h: 1, s: 2, v: 0 });
    }

    #[test]
    fn table_sine_matches_its_table() {
        assert!(table_sin(0.0, 0).abs() < 1e-12);
        assert!((table_sin(FRAC_PI_2, 0) - 1.0).abs() < 1e-6);
        assert!(
            table_sin(FRAC_PI_2, 2048).abs() < 1e-6,
            "the cosine of 90 degrees"
        );
    }

    #[test]
    fn ramp_shape() {
        let ramp = house_ramp(Hsv {
            h: 0,
            s: 255,
            v: 255,
        });
        assert_eq!(ramp[0], [0, 0, 0], "slot 16 is black");
        // From slot 17 the ramp darkens to near black at slot 31.
        let reds: Vec<u8> = ramp[1..].iter().map(|c| c[0]).collect();
        assert!(reds.windows(2).all(|w| w[0] >= w[1]), "{reds:?}");
        assert!(reds[0] > 200 && reds[14] < 10, "{reds:?}");
    }
}
