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

    /// A paratrooper jumping: out of limbo at the flight level, to fall.
    Jump {
        at: u32,
        id: u32,
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
/// Frames from the Weather Storm firing to its first strike on its target.
/// `[observed]` (the cloud forms after `LightningDeferment=` 250)
const STORM_FIRST_HIT: u32 = 323;
/// Paratroopers: frames from firing to the first jump, between jumps, and
/// how far a paratrooper falls a frame. Each lands under the plane (in the
/// game, on a spot of the cell under it), which flies south over the target at 37 leptons a frame; the
/// first jumps 940 leptons short of it (inside `ParadropRadius=`).
/// `[observed]` (eight GIs; which spot of its cell each takes looks random)
const PARA_FIRST: u32 = 179;
const PARA_EVERY: u32 = 5;
const PARA_FALL: i32 = 3;
const PARA_PLANE_SPEED: i32 = 37;
const PARA_FIRST_SHORT: i32 = 940;

/// Superweapon effects due this frame: paratroopers falling and jumping,
/// Brutes standing up, the Dominator, and units arriving through the
/// Chronosphere.
fn effects(world: &mut World, rules: &Rules, frame: u32) {
    // Paratroopers fall. `[observed]`
    for o in &mut world.objects {
        if o.kind == Kind::Infantry && !o.limbo && o.coord.z > 0 && frame > o.placed + 1 {
            o.coord.z = (o.coord.z - PARA_FALL).max(0);
        }
    }
    let due: Vec<Later> = {
        let (now, rest): (Vec<_>, Vec<_>) = world.later.drain(..).partition(|l| match l {
            Later::Spawn { at, .. } | Later::Dominate { at, .. } | Later::Jump { at, .. } => {
                *at <= frame
            }
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
            Later::Jump { id, coord, .. } => {
                if let Some(o) = world.objects.iter_mut().find(|o| o.id == id) {
                    o.limbo = false;
                    o.coord = coord;
                    // It starts falling the frame after next. `[observed]`
                    o.placed = frame;
                }
            }
        }
    }
    let (now, later): (Vec<_>, Vec<_>) = world.warps.drain(..).partition(|w| w.1 <= frame);
    world.warps = later;
    for (id, _, to) in now {
        if let Some(o) = world.objects.iter_mut().find(|o| o.id == id) {
            o.coord = to;
        }
    }
}

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
    effects(world, rules, frame);
    // Damage re-counts its owner's power at once. `[observed]` (a Bio
    // Reactor hit by a Dominator: its output fell the same frame)
    crate::production::power(world, rules, frame);
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
        area_damage(world, rules, frame, s.cell, s.damage, s.warhead);
    }
    world
        .objects
        .retain(|o| o.health > 0 || o.dies_at.is_some());
}

/// A blast round `cell`: `damage` after the warhead's `Verses=` for each
/// object's armour, falling off over its `CellSpread=` (see
/// `production::splash`). A unit takes it by its distance from the cell's
/// centre; a building takes it once for each cell of its foundation within
/// the spread, by that cell's distance, each share truncated. `[observed]`
/// (a Dominator blast took 82 from a Construction Yard: five cells at 6%)
fn area_damage(
    world: &mut World,
    rules: &Rules,
    frame: u32,
    cell: (i16, i16),
    damage: i32,
    warhead: ra2_rules::WarheadId,
) {
    let wh = rules.warhead(warhead);
    let at = cell_centre(cell);
    let splash = |full: i32, d: i32| {
        ra2_rules::production::splash(full, d, wh.cell_spread, wh.percent_at_max)
    };
    for o in &mut world.objects {
        if o.limbo || o.shielded_until > frame {
            continue;
        }
        let t = &rules.technos[o.techno];
        let full = wh.verses[t.armor.index()].of(damage);
        if o.kind == Kind::Building {
            let (w, h) = t.foundation;
            let x0 = (o.coord.x - w * crate::coords::LEPTONS_PER_CELL / 2)
                .div_euclid(crate::coords::LEPTONS_PER_CELL);
            let y0 = (o.coord.y - h * crate::coords::LEPTONS_PER_CELL / 2)
                .div_euclid(crate::coords::LEPTONS_PER_CELL);
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let c = crate::coords::Coord {
                        x: x * crate::coords::LEPTONS_PER_CELL
                            + crate::coords::LEPTONS_PER_CELL / 2,
                        y: y * crate::coords::LEPTONS_PER_CELL
                            + crate::coords::LEPTONS_PER_CELL / 2,
                        z: 0,
                    };
                    o.health -= splash(full, crate::hunt::distance(c, at));
                }
            }
        } else {
            o.health -= splash(full, crate::hunt::distance(o.coord, at));
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

/// The Iron Curtain: within `Range=` cells of the target, infantry die
/// (taken away 15 frames later), everything else is invulnerable for
/// `IronCurtainDuration=`. `[observed]` (a GI on the target died, one two
/// cells off didn't; the invulnerability isn't observed yet)
fn iron_curtain(world: &mut World, rules: &Rules, frame: u32, kind: usize, cell: (i16, i16)) {
    let g = &rules.general;
    let sw = &rules.super_weapons[kind];

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

/// The Weather Storm: its target is struck every `LightningHitDelay=`
/// frames from `STORM_FIRST_HIT` until the storm ends, `LightningDeferment=`
/// and `LightningStormDuration=` after firing. `[observed]` (the random
/// bolts round it use the game's random numbers and aren't modelled)
fn storm(world: &mut World, rules: &Rules, frame: u32, cell: (i16, i16)) {
    let g = &rules.general;
    let Some(warhead) = g.lightning_warhead else {
        return;
    };

    let end = frame + u32::try_from(g.lightning_deferment + g.lightning_duration).unwrap_or(0);
    let step = u32::try_from(g.lightning_hit_delay.max(1)).unwrap_or(1);
    let mut at = frame + STORM_FIRST_HIT;
    while at < end {
        world.strikes.push(crate::world::Strike {
            lands: at,
            cell,
            damage: g.lightning_damage,
            warhead,
        });
        at += step;
    }
}

/// Paratroopers: the side's `…ParaDropInf=` come aboard at once (in limbo),
/// then jump one at a time and fall to the ground. `[observed]`
fn para_drop(
    world: &mut World,
    rules: &Rules,
    frame: u32,
    house: u32,
    american: bool,
    cell: (i16, i16),
) {
    let side = world
        .houses
        .get(usize::try_from(house).unwrap_or(usize::MAX))
        .and_then(|h| rules.countries.iter().find(|c| c.id == h.country))
        .map_or("GDI", |c| c.side.as_str())
        .to_ascii_lowercase();
    let key = if american {
        "Amer"
    } else {
        match side.as_str() {
            "nod" => "Sov",
            "thirdside" => "Yuri",
            _ => "Ally",
        }
    };
    let Some((_, each)) = rules.general.para_drops.iter().find(|(k, _)| k == key) else {
        return;
    };
    let target = cell_centre(cell);
    let mut k = 0u32;
    for (id, count) in each {
        let Some(techno) = rules
            .technos
            .iter()
            .position(|t| t.id.eq_ignore_ascii_case(id))
        else {
            continue;
        };
        for _ in 0..*count {
            let o = world.spawn(rules, techno, house, crate::coords::Coord::default());
            if let Some(obj) = world.objects.iter_mut().find(|x| x.id == o) {
                obj.limbo = true;
            }
            let y = target.y - PARA_FIRST_SHORT
                + PARA_PLANE_SPEED * i32::try_from(PARA_EVERY * k).unwrap_or(0);
            // Where the plane is: we put each under it, not on the spot of
            // its cell the game picks.
            let ground = crate::coords::Coord {
                x: target.x,
                y,
                z: rules.general.flight_level,
            };
            world.later.push(Later::Jump {
                at: frame + PARA_FIRST + PARA_EVERY * k,
                id: o,
                coord: ground,
            });
            k += 1;
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
    for o in &mut world.objects {
        if o.limbo {
            continue;
        }
        let d = crate::hunt::distance(o.coord, at);
        if o.kind != Kind::Building && d <= reach {
            o.owner = house;
        }
    }
    if let Some(warhead) = g.dominator_warhead {
        area_damage(world, rules, frame, cell, g.dominator_damage, warhead);
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
        iron_curtain(world, rules, frame, kind, cell);
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
        let w = rules.weapon(weapon);
        if let Some(warhead) = w.warhead {
            world.strikes.push(crate::world::Strike {
                lands: frame + NUKE_FLIGHT_FRAMES,
                cell,
                damage: w.damage,
                warhead,
            });
        }
    } else if sw.kind.eq_ignore_ascii_case("LightningStorm") {
        storm(world, rules, frame, cell);
    } else if sw.kind.eq_ignore_ascii_case("AmerParaDrop")
        || sw.kind.eq_ignore_ascii_case("ParaDrop")
    {
        para_drop(
            world,
            rules,
            frame,
            house,
            sw.kind.eq_ignore_ascii_case("AmerParaDrop"),
            cell,
        );
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
                  [BuildingTypes]\n1=GATECH\n2=DRAIN\n[GATECH]\nStrength=500\nSuperWeapon=ForceShieldSpecial\n\
                  [DRAIN]\nStrength=500\nPower=-100\n",
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
        // Something that draws power, and nothing to make it.
        world.spawn(&rules, 1, 0, Coord::default());
        update(&mut world, &rules, 1051);
        assert!(world.supers[0].on_hold);
        assert_eq!(world.supers[0].left(1500), 301);
        world.objects.truncate(1);
        update(&mut world, &rules, 2042);
        assert_eq!(world.supers[0].left(2042), 300);
        update(&mut world, &rules, 2342);
        assert!(!world.supers[0].charged);
        update(&mut world, &rules, 2343);
        assert!(world.supers[0].charged);
    }
}
