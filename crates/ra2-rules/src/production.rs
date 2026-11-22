//! How long things take to build. See `docs/spec/production.md`.
//!
//! The game works this out in floating point (doubles, some values held as
//! singles). IEEE arithmetic on doubles and singles is exact and the same on
//! every platform, and the game runs the x87 at 53-bit precision, so doing
//! the same operations here gives the game's results; only integers leave
//! this module.

use crate::{Rules, TechnoType};

/// Steps a factory counts through to finish anything. `[observed]` (53
/// payments, then done a step later, for three costs)
pub const STEPS: i32 = 54;

/// A house's power, as production sees it: produced over drawn, 1 when
/// nothing is short, as a single. `[recomp 0x004FCE30]`
fn power_ratio(output: i32, drain: i32) -> f32 {
    if output >= drain || drain == 0 {
        1.0
    } else if output == 0 {
        0.0
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let r = (f64::from(output) / f64::from(drain)) as f32;
        r
    }
}

/// x86's truncating conversion.
#[allow(clippy::cast_possible_truncation)]
fn ftol(v: f64) -> i32 {
    if v.is_finite() && v.abs() < 2_147_483_648.0 {
        v.trunc() as i32
    } else {
        i32::MIN
    }
}

/// Frames to build `t` for a house with `output`/`drain` power and
/// `factories` factories of its kind. `[recomp 0x006F47A0, 0x00711EE0]`
///
/// 1. `Cost × BuildSpeed × 0.9`, truncated;
/// 2. × the country's multiplier for the kind (1 so far), truncated;
/// 3. × `BuildTimeMultiplier=`, truncated;
/// 4. ÷ the production speed: with power short, `1 − (1 − ratio) ×
///    LowPowerPenaltyModifier`, at least `MinLowPowerProductionSpeed` and
///    at most `MaxLowPowerProductionSpeed`; else 1 (0.01 if it came to 0);
/// 5. × `MultipleFactory=` once per factory beyond the first, truncating
///    each time;
/// 6. walls × `WallBuildSpeedCoefficient=`.
pub fn build_time(rules: &Rules, t: &TechnoType, output: i32, drain: i32, factories: i32) -> i32 {
    let g = &rules.general;
    let base = ftol(f64::from(t.cost) * g.build_speed.value() * 0.9);
    let time = ftol(f64::from(base) * t.build_time_multiplier.value());
    let ratio = power_ratio(output, drain);
    let mut speed = 1.0 - (1.0 - f64::from(ratio)) * g.low_power_penalty.value();
    if speed <= g.min_low_power_speed.value() {
        speed = g.min_low_power_speed.value();
    }
    if ratio < 1.0 && g.max_low_power_speed.value() < speed {
        speed = g.max_low_power_speed.value();
    }
    if speed == 0.0 {
        speed = 0.01;
    }
    let mut time = ftol(f64::from(time) / speed);
    if g.multiple_factory.value() > 0.0 {
        for _ in 1..factories {
            time = ftol(f64::from(time) * g.multiple_factory.value());
        }
    }
    if t.wall && t.kind == crate::Kind::Building {
        time = ftol(f64::from(time) * g.wall_build_speed.value());
    }
    time
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Ini;

    fn rules() -> Rules {
        Rules::from_ini(
            &Ini::parse(
                b"[General]\nBuildSpeed=.7\nMultipleFactory=0.8\nMinLowPowerProductionSpeed=.5\n\
                  MaxLowPowerProductionSpeed=.8\nLowPowerPenaltyModifier=1\n\
                  [BuildingTypes]\n1=GAPOWR\n2=GAPILE\n[InfantryTypes]\n1=E1\n\
                  [GAPOWR]\nCost=800\n[GAPILE]\nCost=500\n[E1]\nCost=200\n",
            ),
            &Ini::parse(b""),
        )
    }

    #[test]
    fn build_times_give_the_recorded_step_lengths() {
        let r = rules();
        let time = |i: usize, f| build_time(&r, &r.technos[i], 200, 10, f);
        // Steps of 9, 5 and 2 frames, as recorded in allied-base.
        assert_eq!([time(0, 1), time(1, 1), time(2, 1)], [504, 315, 126]);
        assert_eq!(
            [time(0, 1) / STEPS, time(1, 1) / STEPS, time(2, 1) / STEPS],
            [9, 5, 2]
        );
        assert_eq!(time(2, 2), 100, "a second barracks: × 0.8");
    }

    #[test]
    fn low_power_slows_production_between_the_limits() {
        let r = rules();
        let e1 = &r.technos[2];
        assert_eq!(build_time(&r, e1, 0, 100, 1), 252, "no power: half speed");
        assert_eq!(
            build_time(&r, e1, 99, 100, 1),
            157,
            "any shortfall: at most 0.8"
        );
        assert_eq!(
            build_time(&r, e1, 60, 100, 1),
            209,
            "60%: the ratio is a float a hair over 0.6"
        );
    }
}
