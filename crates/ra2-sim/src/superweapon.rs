//! Superweapons: which a house has, and their charge. See
//! `docs/spec/superweapons.md`.

use ra2_rules::production::recharge_frames;
use ra2_rules::{Kind, Rules};

use crate::world::World;

/// A superweapon effect that comes after it's fired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Later {
    /// A new object for a house (a mutated soldier).
    Spawn {
        at: u32,
        techno: usize,
        owner: u32,
        coord: crate::coords::Coord,
    },

    /// The Psychic Dominator's effect, round a cell.
    Dominate {
        at: u32,
        house: u32,
        cell: (i16, i16),
    },
}

/// One of a house's superweapons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Super {
    pub house: u32,
    /// Its index in `[SuperWeaponTypes]`.
    pub kind: usize,
    /// The charge timer: the frame it started (`None` while held) and its
    /// length; while held, the length is what was left.
    pub start: Option<u32>,
    pub frames: i32,
    pub charged: bool,
    pub on_hold: bool,
    /// The Chronosphere: aimed at this cell on this frame, waiting for the
    /// Chrono Warp's click.
    pub armed: Option<((i16, i16), u32)>,
}

impl Super {
    /// Frames left to charge at `frame`, as the game reports them.
    pub fn left(&self, frame: u32) -> i32 {
        match self.start {
            None => self.frames,
            Some(s) => {
                (self.frames - i32::try_from(frame.saturating_sub(s)).unwrap_or(i32::MAX)).max(0)
            }
        }
    }
}

/// A cell's centre.
fn cell_centre(cell: (i16, i16)) -> crate::coords::Coord {
    crate::coords::Coord {
        x: i32::from(cell.0) * crate::coords::LEPTONS_PER_CELL
            + crate::coords::LEPTONS_PER_CELL / 2,
        y: i32::from(cell.1) * crate::coords::LEPTONS_PER_CELL
            + crate::coords::LEPTONS_PER_CELL / 2,
        z: 0,
    }
}

/// Frames an infantryman killed by the Iron Curtain lingers. `[observed]`
const INFANTRY_DEATH_FRAMES: u32 = 15;
/// Frames from launching the Nuke to its landing. `[observed]` (one launch,
/// ten cells from its silo; the missile's flight isn't traced)
const NUKE_FLIGHT_FRAMES: u32 = 419;
/// Frames from the Chrono Warp's click to its units reaching the target.
/// `[observed]` (the same over 5 and 28 cells: likely `ChronoDelay=` (60)
/// and two frames)
const CHRONO_FRAMES: u32 = 62;
/// Frames from the Genetic Mutator firing to its victims standing up as
/// Brutes (the mutation's death animation). `[observed]` (61 and 62 for
/// three GIs; we use 61)
const MUTATE_FRAMES: u32 = 61;
/// Frames from the Psychic Dominator firing to its effect
/// (`DominatorFireAtPercentage=` 20% into its animation). `[observed]`
const DOMINATE_FRAMES: u32 = 37;

/// One frame of every house's superweapons, after power is worked out.
///
/// - A building's `SuperWeapon=` comes to its house the frame after the
///   building is placed, its charge timed from the frame it was placed.
///   `[observed]`
/// - A house with none of a superweapon's buildings left loses it.
///   `[hypothesis]`
/// - A powered superweapon (`IsPowered=`) holds while its house is short of
///   power, keeping what's left as of the frame before; when the power is
///   back it carries on, timed from the frame before. `[observed]`
///   (`allied-force-shield`; the hold is `SuperClass +0x70`)
/// - It's charged the frame after its time runs out. `[observed]`
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    land(world, rules, frame);
    world
        .objects
        .retain(|o| o.dies_at.is_none_or(|d| d > frame));
    let due: Vec<Later> = {
        let (now, rest): (Vec<_>, Vec<_>) = world.later.drain(..).partition(|l| match l {
            Later::Spawn { at, .. } | Later::Dominate { at, .. } => *at <= frame,
        });
        world.later = rest;
        now
    };
    for l in due {
        match l {
            Later::Spawn {
                techno,
                owner,
                coord,
                ..
            } => {
                world.spawn(rules, techno, owner, coord);
            }
            Later::Dominate { house, cell, .. } => dominate(world, rules, frame, house, cell),
        }
    }
    let (now, later): (Vec<_>, Vec<_>) = world.warps.drain(..).partition(|w| w.1 <= frame);
    world.warps = later;
    for (id, _, to) in now {
        if let Some(o) = world.objects.iter_mut().find(|o| o.id == id) {
            o.coord = to;
        }
    }
    // What each house's buildings give it, and since when.
    let mut given: Vec<(u32, usize, u32)> = Vec::new();
    for o in &world.objects {
        if o.limbo || o.kind != Kind::Building || frame <= o.placed {
            continue;
        }
        for name in &rules.technos[o.techno].super_weapons {
            if let Some(k) = rules
                .super_weapons
                .iter()
                .position(|s| s.id.eq_ignore_ascii_case(name))
            {
                given.push((o.owner, k, o.placed));
            }
        }
    }
    world
        .supers
        .retain(|s| given.iter().any(|&(h, k, _)| h == s.house && k == s.kind));
    for &(house, kind, placed) in &given {
        if !world
            .supers
            .iter()
            .any(|s| s.house == house && s.kind == kind)
        {
            world.supers.push(Super {
                house,
                kind,
                start: Some(placed),
                frames: recharge_frames(&rules.super_weapons[kind]),
                charged: false,
                on_hold: false,
                armed: None,
            });
        }
    }
    for s in &mut world.supers {
        // Aimed, the Chronosphere shows as charged again from the next
        // frame, until it's warped. `[observed]`
        if let Some((_, at)) = s.armed {
            s.charged = frame > at;
            continue;
        }
        let h = &world.houses[usize::try_from(s.house).unwrap_or(0)];
        let short = h.power_output < h.power_drain;
        if s.charged {
            continue;
        }
        if rules.super_weapons[s.kind].is_powered {
            if short && !s.on_hold {
                s.frames = s.left(frame - 1);
                s.start = None;
                s.on_hold = true;
            } else if !short && s.on_hold {
                s.start = Some(frame - 1);
                s.on_hold = false;
            }
        }
        if let Some(start) = s.start
            && frame > start + u32::try_from(s.frames).unwrap_or(0)
        {
            s.charged = true;
        }
    }
}

/// Strikes that land this frame: the Nuke's blast hits everything within its
/// warhead's `CellSpread=` (see `production::splash`), its `Verses=` by
/// armour. Things it kills go at once. `[observed]` (the radiation it
/// leaves isn't modelled yet)
fn land(world: &mut World, rules: &Rules, frame: u32) {
    let (now, later): (Vec<_>, Vec<_>) = world.strikes.drain(..).partition(|s| s.lands <= frame);
    world.strikes = later;
    for s in now {
        let weapon = rules.weapon(s.weapon);
        let Some(wh) = weapon.warhead.map(|w| rules.warhead(w)) else {
            continue;
        };
        let at = crate::coords::Coord {
            x: i32::from(s.cell.0) * crate::coords::LEPTONS_PER_CELL
                + crate::coords::LEPTONS_PER_CELL / 2,
            y: i32::from(s.cell.1) * crate::coords::LEPTONS_PER_CELL
                + crate::coords::LEPTONS_PER_CELL / 2,
            z: 0,
        };
        for o in &mut world.objects {
            if o.limbo || o.shielded_until > frame {
                continue;
            }
            let full = wh.verses[rules.technos[o.techno].armor.index()].of(weapon.damage);
            let d = crate::hunt::distance(o.coord, at);
            o.health -= ra2_rules::production::splash(full, d, wh.cell_spread, wh.percent_at_max);
        }
    }
    world
        .objects
        .retain(|o| o.health > 0 || o.dies_at.is_some());
}

/// The Chrono Warp's click: the house's aimed Chronosphere starts charging
/// again (from the frame before), infantry within its `Range=` of where it
/// was aimed die (taken away 15 frames later), and the other units there
/// reach the same place relative to `to` after `CHRONO_FRAMES`. Buildings
/// stay. `[observed]` (two Grizzlies, a third out of range, and a GI)
fn warp(world: &mut World, rules: &Rules, frame: u32, house: u32, to: (i16, i16)) {
    let Some(s) = world
        .supers
        .iter_mut()
        .find(|s| s.house == house && s.armed.is_some())
    else {
        return;
    };
    let Some((from, _)) = s.armed.take() else {
        return;
    };
    s.charged = false;
    s.start = Some(frame - 1);
    s.frames = recharge_frames(&rules.super_weapons[s.kind]);
    let reach = rules.super_weapons[s.kind]
        .range
        .of(crate::coords::LEPTONS_PER_CELL);
    let (src, dst) = (cell_centre(from), cell_centre(to));
    for o in &mut world.objects {
        if o.limbo || o.kind == Kind::Building || crate::hunt::distance(o.coord, src) > reach {
            continue;
        }
        if o.kind == Kind::Infantry {
            o.health = 0;
            o.dies_at = Some(frame + INFANTRY_DEATH_FRAMES);
        } else {
            let at = crate::coords::Coord {
                x: o.coord.x - src.x + dst.x,
                y: o.coord.y - src.y + dst.y,
                z: o.coord.z,
            };
            world.warps.push((o.id, frame + CHRONO_FRAMES, at));
        }
    }
}

/// The Genetic Mutator: infantry its warhead reaches (`CellSpread=`, and
/// `Verses=` above nothing for their armour) die at once and stand up as
/// the firing house's Brutes. `[observed]`
fn mutate(world: &mut World, rules: &Rules, frame: u32, house: u32, cell: (i16, i16)) {
    let g = &rules.general;

    if let Some(wh) = g.mutate_warhead.map(|w| rules.warhead(w)) {
        let at = cell_centre(cell);
        let brute = rules
            .technos
            .iter()
            .position(|t| t.id.eq_ignore_ascii_case("BRUTE"));
        let mut gone = Vec::new();
        for o in &world.objects {
            if o.limbo
                || o.kind != Kind::Infantry
                || crate::hunt::distance(o.coord, at) > wh.cell_spread
            {
                continue;
            }
            if wh.verses[rules.technos[o.techno].armor.index()].of(100) > 0 {
                gone.push((o.id, o.coord));
            }
        }
        for (id, coord) in gone {
            world.objects.retain(|o| o.id != id);
            if let Some(techno) = brute {
                world.later.push(Later::Spawn {
                    at: frame + MUTATE_FRAMES,
                    techno,
                    owner: house,
                    coord,
                });
            }
        }
    }
}

/// The Psychic Dominator's effect: units within `DominatorCaptureRange=`
/// cells of the target become the firing house's (`[observed]`: a Grizzly
/// two cells off didn't), and buildings within its warhead's spread take
/// `DominatorDamage=` (its `Verses=` hit only buildings) `[hypothesis]`.
fn dominate(world: &mut World, rules: &Rules, frame: u32, house: u32, cell: (i16, i16)) {
    let g = &rules.general;
    let at = cell_centre(cell);
    let reach = g.dominator_capture_range * crate::coords::LEPTONS_PER_CELL;
    let wh = g.dominator_warhead.map(|w| rules.warhead(w));
    for o in &mut world.objects {
        if o.limbo {
            continue;
        }
        let d = crate::hunt::distance(o.coord, at);
        if o.kind != Kind::Building && d <= reach {
            o.owner = house;
        }
        if let Some(wh) = wh
            && o.shielded_until <= frame
        {
            let full = wh.verses[rules.technos[o.techno].armor.index()].of(g.dominator_damage);
            o.health -= ra2_rules::production::splash(full, d, wh.cell_spread, wh.percent_at_max);
        }
    }
    world
        .objects
        .retain(|o| o.health > 0 || o.dies_at.is_some());
}

/// Fire a house's charged superweapon at `cell`, on the frame the order
/// takes effect: its charge starts again from the frame before.
/// `[observed]` Then its effect:
///
/// - **Force Shield** (`Type=ForceShield`): the house's buildings within
///   `ForceShieldRadius` cells are shielded for `ForceShieldDuration`
///   frames `[hypothesis]` (not observed: nothing attacks them yet), and
///   the house makes no power from the next frame until
///   `ForceShieldBlackoutDuration` frames after the charge restarted.
///   `[observed]` (`allied-force-shield`: out from 2402 to 3399)
pub fn fire(
    world: &mut World,
    rules: &Rules,
    frame: u32,
    house: u32,
    kind: usize,
    cell: (i16, i16),
) {
    if rules.super_weapons[kind]
        .kind
        .eq_ignore_ascii_case("ChronoWarp")
    {
        warp(world, rules, frame, house, cell);
        return;
    }
    let Some(s) = world
        .supers
        .iter_mut()
        .find(|s| s.house == house && s.kind == kind && s.charged)
    else {
        return;
    };
    s.charged = false;
    if rules.super_weapons[kind]
        .kind
        .eq_ignore_ascii_case("ChronoSphere")
    {
        // Aimed: it waits for the Chrono Warp's click. `[observed]`
        s.armed = Some((cell, frame));
        return;
    }
    s.start = Some(frame - 1);
    s.frames = recharge_frames(&rules.super_weapons[kind]);
    let g = &rules.general;
    let sw = &rules.super_weapons[kind];
    if sw.kind.eq_ignore_ascii_case("IronCurtain") {
        // Within `Range=` cells of the target: infantry die (taken away 15
        // frames later), everything else is invulnerable for
        // `IronCurtainDuration=`. `[observed]` (a GI on the target died, one
        // two cells off didn't; the invulnerability isn't observed yet)
        let reach = sw.range.of(crate::coords::LEPTONS_PER_CELL);
        let at = cell_centre(cell);
        let until = frame + u32::try_from(g.iron_curtain_duration).unwrap_or(0);
        for o in &mut world.objects {
            if o.limbo || crate::hunt::distance(o.coord, at) > reach {
                continue;
            }
            if o.kind == Kind::Infantry {
                o.health = 0;
                o.dies_at = Some(frame + INFANTRY_DEATH_FRAMES);
            } else {
                o.shielded_until = until;
            }
        }
    } else if sw.kind.eq_ignore_ascii_case("GeneticConverter") {
        mutate(world, rules, frame, house, cell);
    } else if sw.kind.eq_ignore_ascii_case("PsychicDominator") {
        world.later.push(Later::Dominate {
            at: frame + DOMINATE_FRAMES,
            house,
            cell,
        });
    } else if sw.kind.eq_ignore_ascii_case("MultiMissile")
        && let Some(weapon) = g.nuke_payload
    {
        world.strikes.push(crate::world::Strike {
            lands: frame + NUKE_FLIGHT_FRAMES,
            cell,
            weapon,
        });
    }
    if sw.kind.eq_ignore_ascii_case("ForceShield") {
        let reach = g.force_shield_radius * crate::coords::LEPTONS_PER_CELL;
        let at = crate::coords::Coord {
            x: i32::from(cell.0) * crate::coords::LEPTONS_PER_CELL
                + crate::coords::LEPTONS_PER_CELL / 2,
            y: i32::from(cell.1) * crate::coords::LEPTONS_PER_CELL
                + crate::coords::LEPTONS_PER_CELL / 2,
            z: 0,
        };
        let until = frame + u32::try_from(g.force_shield_duration).unwrap_or(0);
        for o in &mut world.objects {
            if o.owner == house
                && o.kind == Kind::Building
                && !o.limbo
                && crate::hunt::distance(o.coord, at) <= reach
            {
                o.shielded_until = until;
            }
        }
        if let Some(h) = world
            .houses
            .get_mut(usize::try_from(house).unwrap_or(usize::MAX))
        {
            h.blackout = Some((
                frame + 1,
                frame - 1 + u32::try_from(g.force_shield_blackout).unwrap_or(0),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coords::Coord;
    use ra2_rules::Ini;

    #[test]
    fn a_powered_superweapon_holds_on_low_power_and_carries_on() {
        let rules = Rules::from_ini(
            &Ini::parse(
                b"[SuperWeaponTypes]\n1=ForceShieldSpecial\n[ForceShieldSpecial]\nRechargeTime=.5\nIsPowered=yes\n\
                  [BuildingTypes]\n1=GATECH\n[GATECH]\nStrength=500\nSuperWeapon=ForceShieldSpecial\n",
            ),
            &Ini::parse(b""),
        );
        let mut world = World::default();
        world.add_house("British", 0);
        let lab = world.spawn(&rules, 0, 0, Coord::default());
        world.objects[0].placed = 901;
        assert_eq!(lab, 0);
        update(&mut world, &rules, 901);
        assert!(world.supers.is_empty(), "not until the frame after");
        update(&mut world, &rules, 902);
        assert_eq!(world.supers[0].left(902), 449);
        world.houses[0].power_drain = 100;
        update(&mut world, &rules, 1051);
        assert!(world.supers[0].on_hold);
        assert_eq!(world.supers[0].left(1500), 301);
        world.houses[0].power_drain = 0;
        update(&mut world, &rules, 2042);
        assert_eq!(world.supers[0].left(2042), 300);
        update(&mut world, &rules, 2342);
        assert!(!world.supers[0].charged);
        update(&mut world, &rules, 2343);
        assert!(world.supers[0].charged);
    }
}
