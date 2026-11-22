//! The x87's extended precision, for the few formulas where the game keeps
//! intermediate results on the FPU stack: 64-bit mantissas, rounded to
//! nearest even after each multiply or divide. Doubles give different
//! answers there: `1200 × 0.7 × 0.9` is 755.99… on the x87, so a Harrier
//! takes 755 frames, but exactly 756 in doubles. `[observed]` (`allied-air`)
//!
//! Only multiply, divide, add, comparison, truncation and storing as a
//! float are needed; values are positive or zero.

/// `man × 2^exp`, with `man`'s top bit set, or zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct X80 {
    man: u64,
    exp: i32,
}

impl X80 {
    pub const ZERO: X80 = X80 { man: 0, exp: 0 };

    fn normal(man: u64, exp: i32) -> X80 {
        if man == 0 {
            return X80::ZERO;
        }
        let z = man.leading_zeros();
        X80 {
            man: man << z,
            exp: exp - i32::try_from(z).unwrap_or(0),
        }
    }

    /// `fild`: an integer, exactly. Negative values aren't needed.
    pub fn int(v: i32) -> X80 {
        X80::normal(u64::try_from(v.max(0)).unwrap_or(0), 0)
    }

    /// `fld qword`: a double, exactly.
    pub fn double(v: f64) -> X80 {
        let bits = v.to_bits();
        let e = i32::try_from((bits >> 52) & 0x7FF).unwrap_or(0);
        let frac = bits & ((1 << 52) - 1);
        if e == 0 && frac == 0 || bits >> 63 == 1 {
            return X80::ZERO;
        }
        let (man, exp) = if e == 0 {
            (frac, -1074)
        } else {
            (frac | (1 << 52), e - 1075)
        };
        X80::normal(man, exp)
    }

    /// Round `m` (a 128-bit product or quotient whose bits beyond the top
    /// 64 are to go) to 64 bits, nearest even; `sticky` says whether any
    /// bits below `m` were nonzero.
    fn round(m: u128, exp: i32, sticky: bool) -> X80 {
        let bits = 128 - m.leading_zeros();
        if bits <= 64 {
            return X80::normal(u64::try_from(m).unwrap_or(0), exp);
        }
        let shift = bits - 64;
        let kept = u64::try_from(m >> shift).unwrap_or(u64::MAX);
        let rest = m & ((1u128 << shift) - 1);
        let half = 1u128 << (shift - 1);
        let up = rest > half || rest == half && (sticky || kept & 1 == 1);
        let exp = exp + i32::try_from(shift).unwrap_or(0);
        if !up {
            return X80::normal(kept, exp);
        }
        match kept.checked_add(1) {
            Some(k) => X80::normal(k, exp),
            None => X80 {
                man: 1 << 63,
                exp: exp + 1,
            },
        }
    }

    /// `_ftol`: truncated toward zero; out of range gives `i32::MIN`.
    pub fn trunc(self) -> i32 {
        if self.man == 0 {
            return 0;
        }
        if self.exp >= 0 {
            return i32::MIN;
        }
        let shift = u32::try_from(-self.exp).unwrap_or(u32::MAX);
        let v = if shift >= 64 { 0 } else { self.man >> shift };
        i32::try_from(v).unwrap_or(i32::MIN)
    }
}

impl X80 {
    /// `fld dword`: a float, exactly, from its bits.
    pub fn float(bits: u32) -> X80 {
        let e = i32::try_from((bits >> 23) & 0xFF).unwrap_or(0);
        let frac = u64::from(bits & ((1 << 23) - 1));
        if bits >> 31 == 1 || e == 0 && frac == 0 {
            return X80::ZERO;
        }
        if e == 0 {
            return X80::normal(frac, -149);
        }
        X80::normal(frac | (1 << 23), e - 150)
    }

    /// `fstp dword`: rounded to a float, nearest even, as its bits.
    /// Values past a float's range aren't needed.
    pub fn to_float(self) -> u32 {
        if self.man == 0 {
            return 0;
        }
        // man × 2^exp, man's top bit at 63: keep 24 bits.
        let kept = self.man >> 40;
        let rest = self.man & ((1 << 40) - 1);
        let half = 1u64 << 39;
        let up = rest > half || rest == half && kept & 1 == 1;
        let (mut kept, mut exp) = (kept, self.exp + 40);
        if up {
            kept += 1;
            if kept == 1 << 24 {
                kept >>= 1;
                exp += 1;
            }
        }
        // kept × 2^exp, with kept's top bit at 23: biased exponent.
        let biased = exp + 23 + 127;
        if biased <= 0 {
            return 0;
        }
        (u32::try_from(biased).unwrap_or(0xFE) << 23)
            | u32::try_from(kept & ((1 << 23) - 1)).unwrap_or(0)
    }
}

/// `fadd`.
impl std::ops::Add for X80 {
    type Output = X80;
    fn add(self, o: X80) -> X80 {
        if self.man == 0 {
            return o;
        }
        if o.man == 0 {
            return self;
        }
        let (big, small) = if self.exp >= o.exp {
            (self, o)
        } else {
            (o, self)
        };
        let shift = u32::try_from(big.exp - small.exp).unwrap_or(u32::MAX);
        // Both in a 128-bit field, the larger at the top less one bit.
        let a = u128::from(big.man) << 63;
        let (b, sticky) = if shift >= 126 {
            (0, true)
        } else {
            let wide = u128::from(small.man) << 63;
            (wide >> shift, wide & ((1u128 << shift) - 1) != 0)
        };
        X80::round(a + b, big.exp - 63, sticky)
    }
}

/// Comparison, as `fcom` orders positive values.
impl PartialOrd for X80 {
    fn partial_cmp(&self, o: &X80) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for X80 {
    fn cmp(&self, o: &X80) -> std::cmp::Ordering {
        match (self.man == 0, o.man == 0) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => self.exp.cmp(&o.exp).then(self.man.cmp(&o.man)),
        }
    }
}

/// `fmul`.
impl std::ops::Mul for X80 {
    type Output = X80;
    fn mul(self, o: X80) -> X80 {
        if self.man == 0 || o.man == 0 {
            return X80::ZERO;
        }
        X80::round(
            u128::from(self.man) * u128::from(o.man),
            self.exp + o.exp,
            false,
        )
    }
}

/// `fdiv`: `self ÷ o`.
impl std::ops::Div for X80 {
    type Output = X80;
    fn div(self, o: X80) -> X80 {
        if self.man == 0 || o.man == 0 {
            return X80::ZERO;
        }
        // 66 extra bits: the quotient has 65 to 67, enough to round.
        let n = u128::from(self.man) << 63;
        let q = n / u128::from(o.man);
        let r = n % u128::from(o.man);
        // n = man·2^63, so the quotient's scale is 2^(e1 − 63 − e2); widen
        // it by another 3 bits for rounding room.
        let q3 = (q << 3) | ((r << 3) / u128::from(o.man));
        let r3 = (r << 3) % u128::from(o.man);
        X80::round(q3, self.exp - 63 - o.exp - 3, r3 != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn products_keep_64_bits() {
        // In doubles 1200 × 0.7 rounds up to 840; on the x87 it stays below.
        let v = X80::int(1200) * X80::double(0.7) * X80::double(0.9);
        assert_eq!(v.trunc(), 755);
        let w = X80::int(800) * X80::double(0.7) * X80::double(0.9);
        assert_eq!(
            w.trunc(),
            503,
            "the power plant: 503, though its steps are 9 either way"
        );
    }

    #[test]
    fn floats_load_add_and_store() {
        let third = X80::int(3000) / (X80::int(700) * X80::double(3.0));
        assert_eq!(f32::from_bits(third.to_float()), 1.428_571_5);
        assert_eq!(third.to_float(), 0x3FB6_DB6E);
        let more =
            X80::float(third.to_float()) + X80::int(200) / (X80::int(700) * X80::double(3.0));
        assert_eq!(f32::from_bits(more.to_float()), 1.523_809_5);
        assert_eq!(X80::float(1.5f32.to_bits()).to_float(), 1.5f32.to_bits());
        assert!(X80::double(2.0) <= X80::int(2));
        assert!(X80::double(2.0) > X80::float(1.5f32.to_bits()));
    }

    #[test]
    fn exact_values_stay_exact() {
        assert_eq!((X80::int(126) / X80::double(0.5)).trunc(), 252);
        assert_eq!((X80::int(7) * X80::double(2.0)).trunc(), 14);
        assert_eq!((X80::int(100) / X80::int(3)).trunc(), 33);
        assert_eq!((X80::int(0) * X80::double(5.0)).trunc(), 0);
    }
}
