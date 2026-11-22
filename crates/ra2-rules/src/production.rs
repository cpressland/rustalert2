//! How long things take to build. See `docs/spec/production.md`.
//!
//! The game works this out on the x87, in extended precision, from doubles
//! and singles; [`crate::x87`] does the same multiplies and divides with
//! integers, so only integers leave this module and every platform agrees.

use crate::x87::X80;
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

/// Frames to build `t` for a house with `output`/`drain` power and
/// `factories` factories of its kind. `[recomp 0x006F47A0, 0x00711EE0]`
///
/// 1. `Cost × BuildSpeed × 0.9`, truncated (on the x87: a power plant at
///    800 takes 503 frames, not 504);
/// 2. × the country's multiplier for the kind (`BuildTimeInfantryMult=`
///    and so on; defences are buildings with `BuildCat=Combat`), truncated;
/// 3. × `BuildTimeMultiplier=`, truncated;
/// 4. ÷ the production speed: with power short, `1 − (1 − ratio) ×
///    LowPowerPenaltyModifier`, at least `MinLowPowerProductionSpeed` and
///    at most `MaxLowPowerProductionSpeed`; else 1 (0.01 if it came to 0);
/// 5. × `MultipleFactory=` once per factory beyond the first, truncating
///    each time;
/// 6. walls × `WallBuildSpeedCoefficient=`.
pub fn build_time(
    rules: &Rules,
    t: &TechnoType,
    country: Option<&crate::Country>,
    output: i32,
    drain: i32,
    factories: i32,
) -> i32 {
    let g = &rules.general;
    let x = |r: crate::Ratio| X80::double(r.value());
    let base = (X80::int(t.cost) * x(g.build_speed) * X80::double(0.9)).trunc();
    let mult = country.map_or(crate::Ratio::ONE, |c| {
        let i = match t.kind {
            crate::Kind::Infantry => 0,
            crate::Kind::Vehicle => 1,
            crate::Kind::Aircraft => 2,
            crate::Kind::Building if t.combat => 4,
            crate::Kind::Building => 3,
        };
        c.build_time_mults[i]
    });
    let base = (X80::int(base) * x(mult)).trunc();
    let time = (X80::int(base) * x(t.build_time_multiplier)).trunc();
    // The speed's arithmetic is exact (singles and their products), so
    // doubles give it as the x87 does.
    let ratio = power_ratio(output, drain);
    let mut speed = 1.0 - (1.0 - f64::from(ratio)) * g.low_power_penalty.value();
    if speed <= g.min_low_power_speed.value() {
        speed = g.min_low_power_speed.value();
    }
    if ratio < 1.0 && g.max_low_power_speed.value() < speed {
        speed = g.max_low_power_speed.value();
    }
    if speed == 0.0 {
        speed = f64::from(0.01_f32);
    }
    let mut time = (X80::int(time) / X80::double(speed)).trunc();
    if g.multiple_factory.value() > 0.0 {
        for _ in 1..factories {
            time = (X80::int(time) * x(g.multiple_factory)).trunc();
        }
    }
    if t.wall && t.kind == crate::Kind::Building {
        time = (X80::int(time) * x(g.wall_build_speed)).trunc();
    }
    time
}

/// What a building adds to its house's power: its `Power=` × its health
/// ratio, truncated. `[recomp 0x0044E7B0]` (drawing power isn't scaled:
/// `0x0044E880`)
pub fn power_output(power: i32, health: i32, strength: i32) -> i32 {
    if power <= 0 || strength <= 0 {
        return 0;
    }
    (X80::int(health) / X80::int(strength) * X80::int(power)).trunc()
}

/// What selling `t` pays a human player's house: `Soylent=` if set, else
/// its cost × `RefundPercent=`, truncated (the country multipliers are 1
/// so far). `[recomp 0x00711F60]`
pub fn refund(rules: &Rules, t: &TechnoType) -> i32 {
    if t.soylent != 0 {
        return t.soylent;
    }
    (X80::int(t.cost) * X80::double(rules.general.refund_percent.value())).trunc()
}

/// Frames a superweapon takes to charge: `RechargeTime=` (minutes) × 900,
/// truncated; 4500 (five minutes) if it's 0. `[recomp 0x006CED6E,
/// 0x006CE5E9]`
pub fn recharge_frames(sw: &crate::SuperWeaponType) -> i32 {
    if sw.recharge_minutes.value() == 0.0 {
        return 4500;
    }
    (X80::double(sw.recharge_minutes.value()) * X80::double(900.0)).trunc()
}

/// A blast's damage `distance` leptons from where it lands: `damage` (after
/// the warhead's `Verses=`) × (1 − (1 − `PercentAtMax=`) × distance ÷
/// `CellSpread=`), truncated; nothing beyond the spread. `[observed]`
/// (the Nuke: 247 of 600 at 6 cells, 70 at 9)
pub fn splash(damage: i32, distance: i32, spread: i32, percent_at_max: crate::Ratio) -> i32 {
    if spread <= 0 || distance > spread {
        return 0;
    }
    let falloff = 1.0 - (1.0 - percent_at_max.value()) * f64::from(distance) / f64::from(spread);
    (X80::int(damage) * X80::double(falloff)).trunc()
}

/// Frames between repair steps, and self-healing's: `RepairRate=`
/// (minutes) × 900, truncated (0.016 gives 14). `[observed]` (both step on
/// the same frames, 1 + 14k, as if on one game-wide clock)
pub fn repair_frames(rules: &Rules) -> i32 {
    (X80::double(rules.general.repair_rate.value()) * X80::double(900.0))
        .trunc()
        .max(1)
}

/// What one repair step of `t` costs: its cost × `RepairPercent=` ×
/// `RepairStep=` ÷ `Strength=`, truncated. `[observed]` (a Bio Reactor,
/// 600 credits and 700 hit points: 1 credit for each 8)
pub fn repair_step_cost(rules: &Rules, t: &TechnoType) -> i32 {
    if t.strength <= 0 {
        return 0;
    }
    (X80::int(t.cost)
        * X80::double(rules.general.repair_percent.value())
        * X80::int(rules.general.repair_step)
        / X80::int(t.strength))
    .trunc()
}

/// Experience after a kill worth `worth` (the victim's cost; twice that
/// for a veteran victim, three times for elite) by a unit costing `cost`,
/// from `experience` (a float's bits) to the new float's bits: worth ÷
/// (cost × `VeteranRatio=`) added on, capped at `VeteranCap=`, all on the
/// x87. `[recomp 0x0074FF50]` (`grizzly-veteran`: 3000 ÷ 2100 gave
/// 1.42857146)
pub fn experience_after_kill(rules: &Rules, experience: u32, worth: i32, cost: i32) -> u32 {
    let g = &rules.general;
    if cost <= 0 {
        return experience;
    }
    let total = X80::float(experience)
        + X80::int(worth) / (X80::int(cost) * X80::double(g.veteran_ratio.value()));
    let cap = X80::double(g.veteran_cap.value());
    if cap <= total { cap } else { total }.to_float()
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
        let time = |i: usize, f| build_time(&r, &r.technos[i], None, 200, 10, f);
        // Steps of 9, 5 and 2 frames, as recorded in allied-base.
        assert_eq!([time(0, 1), time(1, 1), time(2, 1)], [503, 314, 125]);
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
        assert_eq!(
            build_time(&r, e1, None, 0, 100, 1),
            250,
            "no power: half speed"
        );
        assert_eq!(
            build_time(&r, e1, None, 99, 100, 1),
            156,
            "any shortfall: at most 0.8"
        );
        assert_eq!(
            build_time(&r, e1, None, 60, 100, 1),
            208,
            "60%: the ratio is a float a hair over 0.6"
        );
    }

    #[test]
    fn a_country_multiplier_scales_its_kind_only() {
        let r = Rules::from_ini(
            &Ini::parse(
                b"[General]\nBuildSpeed=.7\n[Countries]\n1=British\n\
                  [British]\nBuildTimeInfantryMult=.75\n\
                  [InfantryTypes]\n1=E1\n[BuildingTypes]\n1=GAPOWR\n\
                  [E1]\nCost=200\n[GAPOWR]\nCost=800\n",
            ),
            &Ini::parse(b""),
        );
        let gb = r.countries.first();
        let id = |n: &str| r.technos.iter().position(|t| t.id == n).expect("type");
        assert_eq!(
            build_time(&r, &r.technos[id("E1")], gb, 0, 0, 1),
            93,
            "125 x 0.75"
        );
        assert_eq!(build_time(&r, &r.technos[id("GAPOWR")], gb, 0, 0, 1), 503);
    }
}
