//! What a destroyed unit leaves: the blast of its death weapon, and its
//! crew. See `docs/spec/combat.md` (death).

use ra2_rules::x87::X80;
use ra2_rules::{Kind, Rules};

use crate::coords::Coord;
use crate::rng::Randomizer;
use crate::world::World;

/// A wreck's blast, going off on `lands`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wreck {
    pub at: Coord,
    pub lands: u32,
    pub damage: i32,
    pub warhead: ra2_rules::WarheadId,
}

/// 1 ÷ (2³¹ − 2): a draw from 0 to 2³¹ − 2 times this is a fraction.
/// `[recomp 0x007E3570]`
const FRACTION: u64 = 0x3E00_0000_4000_0000;

/// A vehicle at `i` has just been destroyed on `frame`. Its death weapon
/// goes off where it stood the frame after; its crew, if it has one, may
/// get out first. `[recomp 0x0070D690, 0x007381F7]` (`[observed]`: an
/// MCV's crewman appeared on the frame it was destroyed and was killed by
/// the blast on the next)
pub(crate) fn destroyed(
    world: &mut World,
    rules: &Rules,
    rng: &mut Randomizer,
    i: usize,
    frame: u32,
) {
    let o = &world.objects[i];
    if o.kind != Kind::Vehicle {
        return;
    }
    let t = &rules.technos[o.techno];
    let (at, owner) = (o.coord, o.owner);
    if let Some((damage, warhead)) = death_blast(rules, t) {
        world.wrecks.push(Wreck {
            at,
            lands: frame + 1,
            damage,
            warhead,
        });
    }
    if t.crewed {
        crew(world, rules, rng, at, owner);
    }
}

/// The death weapon's damage and warhead: its own `DeathWeapon=` at its
/// damage × `DeathWeaponDamageModifier=`; else its primary weapon's, the
/// same way; else `[CombatDamage] DeathWeapon=` at half its `Strength=`.
/// Each truncated. `[recomp 0x0070D690]`
fn death_blast(rules: &Rules, t: &ra2_rules::TechnoType) -> Option<(i32, ra2_rules::WarheadId)> {
    if let Some(w) = t.death_weapon.or(t.primary) {
        let weapon = rules.weapon(w);
        let damage =
            (X80::int(weapon.damage) * X80::double(t.death_weapon_damage_modifier.value())).trunc();
        return weapon.warhead.map(|wh| (damage, wh));
    }
    let weapon = rules.weapon(rules.general.default_death_weapon?);
    let damage = (X80::int(t.strength) * X80::double(0.5)).trunc();
    weapon.warhead.map(|wh| (damage, wh))
}

/// A wreck's crew: out if a draw from 0 to 2³¹ − 2, as a fraction, is
/// below `CrewEscape=`. One of the side's crew, where the wreck stood, with
/// 5 to half its strength, at random. `[recomp 0x007381F7, 0x00738302]`
fn crew(world: &mut World, rules: &Rules, rng: &mut Randomizer, at: Coord, owner: u32) {
    let r = rng.ranged(0, 0x7FFF_FFFE);
    let fraction = X80::int(r) * X80::double_bits(FRACTION);
    if fraction >= X80::double(rules.general.crew_escape.value()) {
        return;
    }
    let Some(house) = world
        .houses
        .get(usize::try_from(owner).unwrap_or(usize::MAX))
    else {
        return;
    };
    let side = rules
        .countries
        .iter()
        .find(|c| c.id.eq_ignore_ascii_case(&house.country))
        .map_or(0, |c| match c.side.to_ascii_lowercase().as_str() {
            "nod" => 1,
            "thirdside" => 2,
            _ => 0,
        });
    let country = house.country.clone();
    let crew_type = &rules.general.crews[side];
    let Some(techno) = rules
        .technos
        .iter()
        .position(|t| t.id.eq_ignore_ascii_case(crew_type))
    else {
        return;
    };
    let p = ra2_rules::scenario::Placement {
        kind: Kind::Infantry,
        type_id: rules.technos[techno].id.clone(),
        owner: country,
        cell: crate::hunt::to_cell(at),
        sub_cell: 0,
        health: 256,
        facing: 0,
        mission: "Guard".into(),
        veterancy: 0,
    };
    if world.place(rules, &p, 0).is_some()
        && let Some(o) = world.objects.last_mut()
    {
        o.owner = owner;
        o.coord = Coord { z: 0, ..at };
        o.health = rng.ranged(5, rules.technos[techno].strength / 2);
    }
}

/// The wrecks' blasts that go off on `frame`, and buildings whose C4 goes
/// off.
pub(crate) fn update(world: &mut World, rules: &Rules, frame: u32) {
    let before = world.objects.len();
    world
        .objects
        .retain(|o| o.c4_at.is_none_or(|at| at > frame));
    if world.objects.len() != before {
        crate::production::power(world, rules, frame);
    }
    let (now, later): (Vec<_>, Vec<_>) = world.wrecks.drain(..).partition(|w| w.lands <= frame);
    world.wrecks = later;
    for w in now {
        crate::superweapon::blast(world, rules, frame, w.at, w.damage, w.warhead, true);
    }
}
