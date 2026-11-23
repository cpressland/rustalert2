//! Crates: a unit that drives onto one gets its powerup. See
//! `docs/spec/repair.md` (crates).

use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::rng::Randomizer;
use crate::world::World;

/// A powerup's index in `ra2_rules::POWERUPS`.
const MONEY: u8 = 0;
const UNIT: u8 = 1;
const HEAL_BASE: u8 = 2;
const ARMOR: u8 = 9;
const SPEED: u8 = 10;
const FIREPOWER: u8 = 11;
const VETERAN: u8 = 14;
/// A crate's data of this or more means a pick by weight.
const RANDOM: u8 = 19;

/// Put the map's one crate at `cell`, holding `kind`. (The recorder moves
/// the game's random crate the same way.)
pub fn place(world: &mut World, cell: (i16, i16), kind: u8) {
    world.crates.clear();
    world.crates.insert(cell, kind);
}

/// The unit `i` is setting off into `cell`: if there's a crate there, it
/// takes it, on that frame. True if the unit must stop short (a unit crate
/// puts its unit on the crate's cell). `[observed]` (`mcv-crates`: each
/// pickup on the frame the MCV started its step into the crate's cell)
pub(crate) fn enter(
    world: &mut World,
    rules: &Rules,
    rng: &mut Randomizer,
    i: usize,
    cell: (i16, i16),
) -> bool {
    let Some(kind) = world.crates.remove(&cell) else {
        return false;
    };
    let kind = if kind >= RANDOM {
        pick(rules, rng)
    } else {
        kind
    };
    open(world, rules, rng, i, cell, kind)
}

/// A pick by `[Powerups]` weight: a number from 1 to their sum, and the
/// first powerup whose running total reaches it. `[recomp 0x00481AD3]`
fn pick(rules: &Rules, rng: &mut Randomizer) -> u8 {
    let total: i32 = rules.crates.powerups.iter().map(|p| p.weight).sum();
    if total <= 0 {
        return MONEY;
    }
    let r = rng.ranged(1, total);
    let mut sum = 0;
    for (k, p) in rules.crates.powerups.iter().enumerate() {
        sum += p.weight;
        if r <= sum {
            return u8::try_from(k).unwrap_or(MONEY);
        }
    }
    MONEY
}

fn open(
    world: &mut World,
    rules: &Rules,
    rng: &mut Randomizer,
    i: usize,
    cell: (i16, i16),
    kind: u8,
) -> bool {
    let owner = world.objects[i].owner;
    let data = |k: u8| rules.crates.powerups[usize::from(k)].data;
    let centre = Coord {
        x: i32::from(cell.0) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        y: i32::from(cell.1) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        z: 0,
    };
    let near = |o: &crate::world::Object| {
        o.owner == owner
            && !o.limbo
            && o.kind != Kind::Building
            && crate::hunt::distance(o.coord, centre) <= rules.crates.radius
    };
    match kind {
        UNIT => {
            if let Some(t) = rules.crates.unit_type.as_deref()
                && let Some(techno) = rules
                    .technos
                    .iter()
                    .position(|x| x.id.eq_ignore_ascii_case(t))
            {
                let country = world
                    .houses
                    .get(usize::try_from(owner).unwrap_or(usize::MAX))
                    .map(|h| h.country.clone())
                    .unwrap_or_default();
                let p = ra2_rules::scenario::Placement {
                    kind: rules.technos[techno].kind,
                    type_id: rules.technos[techno].id.clone(),
                    owner: country,
                    cell,
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
                }
                return true;
            }
            money(world, rng, owner, data(MONEY).of(1));
        }
        HEAL_BASE => {
            for o in &mut world.objects {
                if o.owner == owner && o.kind == Kind::Building {
                    o.health = rules.technos[o.techno].strength;
                }
            }
        }
        SPEED | ARMOR | FIREPOWER => {
            let by = data(kind);
            for o in world.objects.iter_mut().filter(|o| near(o)) {
                match kind {
                    SPEED => o.speed_bonus = by,
                    ARMOR => o.armor_bonus = by,
                    _ => o.firepower_bonus = by,
                }
            }
        }
        VETERAN => {
            // Each trainable unit near gains the crate's levels; with none,
            // it's money. `[observed]` (an MCV alone got money)
            let levels = data(VETERAN).of(1);
            let mut any = false;
            for o in &mut world.objects {
                if near(o) && rules.technos[o.techno].trainable {
                    any = true;
                    let worth = levels.max(0);
                    o.experience_bits =
                        ra2_rules::veterancy::promote(rules, o.experience_bits, worth);
                }
            }
            if !any {
                money(world, rng, owner, data(MONEY).of(1));
            }
        }
        // Money, and the powerups not modelled yet. `[hypothesis]`
        _ => money(world, rng, owner, data(MONEY).of(1)),
    }
    false
}

/// A money crate: its amount plus 0 to 900, at random. `[recomp
/// 0x00482484]` (`[observed]`: 2879 and 2412 from 2000)
fn money(world: &mut World, rng: &mut Randomizer, owner: u32, amount: i32) {
    let got = rng.ranged(amount, amount + 900);
    if let Some(h) = world
        .houses
        .get_mut(usize::try_from(owner).unwrap_or(usize::MAX))
    {
        h.credits += got;
    }
}
