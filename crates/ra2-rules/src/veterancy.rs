//! What veterancy does. A unit's rank comes from its experience (see
//! `production::experience_after_kill`): a veteran from 1, elite from 2.
//! Each of its type's abilities (`VeteranAbilities=`, and for an elite
//! unit `EliteAbilities=` too) changes one number, by a `[General]`
//! factor, truncated. `[recomp 0x0070D0D0]`
//!
//! | Ability | What | Factor | Where |
//! |---|---|---|---|
//! | `FIREPOWER` | damage it deals, × | `VeteranCombat=` | `[recomp 0x006FDC4B]` |
//! | `STRONGER` | damage it takes, ÷ (at least 1) | `VeteranArmor=` | `[recomp 0x007019CB]` |
//! | `ROF` | its reload, × | `VeteranROF=` | `[recomp 0x006FD13F]` |
//! | `FASTER` | its speed, × | `VeteranSpeed=` | `[recomp 0x004DB1FA]` |

use crate::x87::X80;
use crate::{Rules, TechnoType};

/// Whether a unit of type `t` and `rank` (0 rookie, 1 veteran, 2 elite)
/// has `ability`.
pub fn has(t: &TechnoType, rank: u8, ability: &str) -> bool {
    let named = |list: &[String]| list.iter().any(|a| a.eq_ignore_ascii_case(ability));
    match rank {
        0 => false,
        1 => named(&t.veteran_abilities),
        _ => named(&t.veteran_abilities) || named(&t.elite_abilities),
    }
}

/// The damage a shot deals.
pub fn firepower(rules: &Rules, t: &TechnoType, rank: u8, damage: i32) -> i32 {
    if !has(t, rank, "FIREPOWER") {
        return damage;
    }
    (X80::int(damage) * X80::double(rules.general.veteran_combat.value())).trunc()
}

/// The damage taken from a hit.
pub fn armor(rules: &Rules, t: &TechnoType, rank: u8, damage: i32) -> i32 {
    if !has(t, rank, "STRONGER") || damage <= 0 {
        return damage;
    }
    (X80::int(damage) / X80::double(rules.general.veteran_armor.value()))
        .trunc()
        .max(1)
}

/// Frames between shots.
pub fn rof(rules: &Rules, t: &TechnoType, rank: u8, rof: i32) -> i32 {
    if !has(t, rank, "ROF") {
        return rof;
    }
    (X80::int(rof) * X80::double(rules.general.veteran_rof.value())).trunc()
}

/// The factor on its speed, as a ratio (1 without `FASTER`).
pub fn speed_factor(rules: &Rules, t: &TechnoType, rank: u8) -> crate::Ratio {
    if has(t, rank, "FASTER") {
        rules.general.veteran_speed
    } else {
        crate::Ratio::ONE
    }
}

/// The experience a map's `Veterancy=` field starts a unit with: 100 a
/// veteran, 200 elite, as a float's bits. `[observed]` (100 gave 1)
pub fn from_map(veterancy: i32) -> u32 {
    (X80::int(veterancy.max(0)) / X80::int(100)).to_float()
}
