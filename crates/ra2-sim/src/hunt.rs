//! The Hunt mission: pick the nearest enemy, turn, drive or walk until it's
//! in range, and fire on each reload. See `docs/spec/locomotors.md` and
//! `docs/spec/combat.md`; the timings are fitted to the parity reference
//! logs and marked as such there.

use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::pathfind;
use crate::rng::Randomizer;
use crate::world::{Mission, Object, Shot, World};

/// Leptons a frame, in 1/256ths, per point of a type's stored speed:
/// vehicles about 1.56, infantry about 1.05. `[observed]`
pub(crate) const DRIVE_SPEED: i32 = 406;
pub(crate) const WALK_SPEED: i32 = 277;
/// Frames more a mind control link takes to land. `[observed]`
/// (`yuri-mind-control`: Yuri fired the frame after he stopped, 1,739
/// leptons off, and the Grizzly was his 5 frames later)
const MIND_CONTROL_FLIGHT: i32 = 4;
/// Leptons of distance per frame of an arcing shell's flight. `[observed]`
const ARC_LEPTONS_PER_FRAME: i32 = 128;
/// A full turn in the 16-bit facing the game keeps.
pub(crate) const FULL_TURN: i32 = 65_536;

/// Spots infantry stop on, as offsets in a cell (see `world::SUB_CELLS`).
const SPOTS: [(i32, i32); 5] = [(128, 128), (64, 64), (192, 64), (64, 192), (192, 192)];

/// Where an object is going and how far along it is.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Motion {
    /// The object it hunts.
    pub target: Option<u32>,
    /// The planned route still to drive, as directions (see
    /// `pathfind::DIRECTIONS`), planned once toward its attack position.
    pub route: Vec<u8>,
    /// The leg in progress: from, to, and leptons travelled along it.
    pub leg: Option<(Coord, Coord, i32)>,
    /// Sub-lepton progress, in 1/256ths.
    pub carry: i32,
    /// It has moved since it last stood still (so turns at corners happen
    /// on the move).
    pub rolling: bool,
    /// It reached its firing spot.
    pub arrived: bool,
    /// The frame it arrived on.
    pub arrived_at: u32,
    /// Frames until its weapon is ready.
    pub reload: i32,
    /// Shots fired of its weapon's `Burst=`.
    pub burst: i32,
    /// How much of its full speed it's doing, in millionths; 0 before it
    /// first moves. See [`throttle`].
    pub speed: i32,
    /// The cell centre its route ends at.
    pub goal: Option<Coord>,
    /// The cell a commanded unit's route was planned to.
    pub planned: Option<(i16, i16)>,
}

/// Full speed, in the millionths [`Motion::speed`] keeps.
const FULL: i32 = 1_000_000;
/// The slowest a driver slows to, and starts at. `[recomp 0x004B10D0]`
/// (0.3 as a float; starting at it is `[observed]`)
const CRAWL: i32 = 300_000;

/// A factor in millionths, rounded.
fn micro(r: ra2_rules::Ratio) -> i32 {
    (r.of(2_000_000) + 1) / 2
}

/// This frame's speed for a driver, in 1/256 leptons, `dist` leptons from
/// where it's going. Units that don't accelerate go at full speed. Others
/// slow by `DeaccelerationFactor × speed_fraction` a frame, to no less than
/// a crawl, once within `SlowdownDistance`; else they gain
/// `AccelerationFactor` a frame up to full speed. What it moves is the
/// type's speed times that, truncated, in the same units as a full-speed
/// driver. `[recomp 0x004B0F69]` (the speed is a double in the game; we
/// keep millionths, so a frame now and then may differ by one unit)
pub(crate) fn throttle(rules: &Rules, o: &mut Object, t: &ra2_rules::TechnoType, dist: i32) -> i32 {
    let top = object_speed(rules, t, o);
    let m = &mut o.motion;
    if t.accelerates {
        if m.speed < CRAWL {
            m.speed = CRAWL;
        } else if dist < t.slowdown_distance {
            let by = i32::try_from(
                i64::from(micro(t.deacceleration_factor)) * i64::from(t.speed_fraction),
            )
            .unwrap_or(FULL);
            m.speed = (m.speed - by).max(CRAWL);
        } else {
            m.speed = (m.speed + micro(t.acceleration_factor)).min(FULL);
        }
    } else {
        m.speed = FULL;
    }
    let units = i64::from(top) * i64::from(m.speed) / i64::from(FULL);
    i32::try_from(units).unwrap_or(0) * DRIVE_SPEED
}

/// An object's full speed: its type's × a speed crate's bonus, truncated,
/// then × `VeteranSpeed=` if it has `FASTER`. `[recomp 0x004DB1D5]`
/// (`[observed]`: an MCV's 10 became 12 after a 1.2 speed crate)
pub(crate) fn object_speed(rules: &Rules, t: &ra2_rules::TechnoType, o: &Object) -> i32 {
    let base = o.speed_bonus.of(t.speed_fraction);
    ra2_rules::veterancy::speed_factor(rules, t, o.rank()).of(base)
}

fn isqrt(v: i64) -> i32 {
    i32::try_from(u64::try_from(v.max(0)).unwrap_or(0).isqrt()).unwrap_or(i32::MAX)
}

/// Distance between two points on the ground, truncated.
pub fn distance(a: Coord, b: Coord) -> i32 {
    let (dx, dy) = (i64::from(a.x - b.x), i64::from(a.y - b.y));
    isqrt(dx * dx + dy * dy)
}

/// The facing (16-bit, 0 is north, i.e. −y, clockwise) of a one-cell step.
pub(crate) fn step_facing(dx: i32, dy: i32) -> i32 {
    match (dx.signum(), dy.signum()) {
        (0, -1) => 0,
        (1, -1) => 8192,
        (1, 0) => 16_384,
        (1, 1) => 24_576,
        (0, 1) => 32_768,
        (-1, 1) => 40_960,
        (-1, 0) => 49_152,
        _ => 57_344,
    }
}

/// The facing (16 bits, 0 north, clockwise) from one point toward another,
/// by an integer CORDIC: exact on the eight principal directions, within a
/// unit or two elsewhere. `[hypothesis]` (the game takes an arctangent in
/// doubles)
pub(crate) fn facing_to(from: Coord, to: Coord) -> i32 {
    const ATAN: [i64; 15] = [
        8192, 4836, 2555, 1297, 651, 326, 163, 81, 41, 20, 10, 5, 3, 1, 1,
    ];
    // Clockwise from north: x east, y south, so north is (0, -1).
    let (mut x, mut y) = (i64::from(-(to.y - from.y)), i64::from(to.x - from.x));
    if x == 0 && y == 0 {
        return 0;
    }
    // The axes and diagonals exactly.
    if x == 0 || y == 0 || x.abs() == y.abs() {
        return step_facing(to.x - from.x, to.y - from.y);
    }
    let mut angle: i64 = 0;
    // Into the right half-plane first.
    if x < 0 {
        angle = 32_768;
        x = -x;
        y = -y;
    }
    x <<= 16;
    y <<= 16;
    for (k, a) in ATAN.iter().enumerate() {
        let (nx, ny);
        if y > 0 {
            nx = x + (y >> k);
            ny = y - (x >> k);
            angle += a;
        } else {
            nx = x - (y >> k);
            ny = y + (x >> k);
            angle -= a;
        }
        x = nx;
        y = ny;
    }
    i32::try_from(angle.rem_euclid(i64::from(FULL_TURN))).unwrap_or(0)
}

/// Turn `from` toward `to` by at most `rate`, the short way round.
pub(crate) fn turn(from: i32, to: i32, rate: i32) -> i32 {
    let diff = (to - from).rem_euclid(FULL_TURN);
    let diff = if diff > FULL_TURN / 2 {
        diff - FULL_TURN
    } else {
        diff
    };
    if diff.abs() <= rate {
        to
    } else {
        (from + rate * diff.signum()).rem_euclid(FULL_TURN)
    }
}

pub(crate) fn to_cell(c: Coord) -> (i16, i16) {
    (
        i16::try_from(c.x.div_euclid(LEPTONS_PER_CELL)).unwrap_or(0),
        i16::try_from(c.y.div_euclid(LEPTONS_PER_CELL)).unwrap_or(0),
    )
}

pub(crate) fn cell_centre(c: Coord) -> Coord {
    Coord {
        x: c.x.div_euclid(LEPTONS_PER_CELL) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        y: c.y.div_euclid(LEPTONS_PER_CELL) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        z: c.z,
    }
}

/// Advance along the current leg; returns whether the leg finished.
pub(crate) fn travel(o: &mut Object, speed_256: i32) -> bool {
    let Some((from, to, done)) = o.motion.leg else {
        return true;
    };
    let length = distance(from, to).max(1);
    o.motion.carry += speed_256;
    let step = o.motion.carry / 256;
    o.motion.carry %= 256;
    let done = done + step;
    if done >= length {
        // Legs end exactly on their spot, so the step that reaches a cell
        // centre is short; what's left over carries into the next frame.
        o.motion.carry += (done - length) * 256;
        o.coord = to;
        o.motion.leg = None;
        return true;
    }
    o.coord = Coord {
        x: from.x + (to.x - from.x) * done / length,
        y: from.y + (to.y - from.y) * done / length,
        z: from.z,
    };
    o.motion.leg = Some((from, to, done));
    false
}

/// One frame of every hunting object, then the shells that land this frame.
pub fn update(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32) {
    for i in 0..world.objects.len() {
        let o = &world.objects[i];
        let attacking = matches!(o.command, Some(crate::command::Command::Attack(_)));
        if o.mission == Mission::Guard && o.command.is_none() && !o.limbo {
            guard(world, rules, rng, frame, i);
            continue;
        }
        if o.mission != Mission::Hunt && !attacking {
            continue;
        }
        if let Some(crate::command::Command::Attack(id)) = o.command
            && rules.technos[o.techno].c4
            && world
                .objects
                .iter()
                .any(|b| b.id == id && b.kind == Kind::Building)
        {
            crate::command::sapper(world, rules, frame, i, id);
            continue;
        }
        hunt(world, rules, rng, frame, i);
    }
    land(world, rules, rng, frame);
    crate::death::update(world, rules, frame);
    release(world);
}

/// The first frames a unit on Guard can take a target. `[observed]`
/// (`guard-mcv`, three seeds: a GI's first hit landed on 48, fired the
/// frame after it took the target; a Grizzly's on 68, after a 13-frame
/// turret turn and a 9-frame shell)
const GUARD_FROM_INFANTRY: u32 = 46;
const GUARD_FROM_VEHICLE: u32 = 47;

/// Guard: an armed unit takes the nearest living enemy within its
/// weapon's range, keeps it while it lives and stays in range, and fires
/// on it without moving. Enemies out of range are left alone, even within
/// sight. `[observed]` (`guard-mcv`: Grizzlies 9 and 14 cells off stayed
/// put)
fn guard(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32, i: usize) {
    let me = &world.objects[i];
    let t = &rules.technos[me.techno];
    let infantry = me.kind == Kind::Infantry;
    if me.kind == Kind::Building || me.health <= 0 {
        return;
    }
    let Some(weapon) = weapon_now(world, rules, me).map(|w| rules.weapon(w)) else {
        return;
    };
    if weapon.damage < 0 {
        return;
    }
    let from = if infantry {
        GUARD_FROM_INFANTRY
    } else {
        GUARD_FROM_VEHICLE
    };
    if frame < from {
        return;
    }
    let (at, owner) = (me.coord, me.owner);
    let in_range = |o: &crate::world::Object| {
        o.owner != owner && !o.limbo && o.health > 0 && distance(o.coord, at) <= weapon.range
    };
    let target = me
        .motion
        .target
        .and_then(|id| world.objects.iter().find(|o| o.id == id && in_range(o)))
        .or_else(|| {
            world
                .objects
                .iter()
                .filter(|o| in_range(o))
                .min_by_key(|o| (i64::from(distance(o.coord, at)), o.id))
        })
        .map(|o| (o.id, o.coord));
    let o = &mut world.objects[i];
    o.motion.reload = (o.motion.reload - 1).max(0);
    let Some((target_id, target_at)) = target else {
        o.motion.target = None;
        return;
    };
    let fresh = o.motion.target != Some(target_id);
    o.motion.target = Some(target_id);
    if fresh && infantry {
        // Infantry fire from the frame after they take a target.
        o.motion.arrived_at = frame;
        return;
    }
    let aimed = if t.turret {
        let want = facing_to(o.coord, target_at);
        let now = turn(i32::from(o.turret.unwrap_or(o.dir)), want, t.rot * 256);
        o.turret = Some(u16::try_from(now).unwrap_or(0));
        now == want
    } else if infantry {
        o.dir = u16::try_from(facing_to(o.coord, target_at)).unwrap_or(0);
        true
    } else {
        let want = facing_to(o.coord, target_at);
        let now = turn(i32::from(o.dir), want, t.rot * 256);
        o.dir = u16::try_from(now).unwrap_or(0);
        now == want
    };
    if aimed && o.motion.reload == 0 {
        fire(world, rules, rng, frame, i, target_id, infantry);
    }
}

#[allow(clippy::too_many_lines)]
fn hunt(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32, i: usize) {
    let me = world.objects[i].clone();
    let t = &rules.technos[me.techno];
    let Some(weapon) = weapon_now(world, rules, &me).map(|w| rules.weapon(w)) else {
        return;
    };
    // An attack order's target: once it's gone (or, for a mending weapon,
    // whole), the unit stands.
    if let Some(crate::command::Command::Attack(id)) = me.command
        && !world.objects.iter().any(|o| {
            o.id == id
                && o.health > 0
                && (weapon.damage >= 0 || o.health < rules.technos[o.techno].strength)
        })
    {
        let o = &mut world.objects[i];
        o.command = None;
        o.mission = Mission::Guard;
        o.motion.target = None;
        return;
    }
    // The target: kept while it lives, else the nearest object of another
    // house. `[hypothesis]` (threat and house alliances not yet weighed)
    let ordered = match me.command {
        Some(crate::command::Command::Attack(id)) => Some(id),
        _ => None,
    };
    let target = ordered
        .or(me.motion.target)
        .and_then(|id| world.objects.iter().find(|o| o.id == id))
        .or_else(|| {
            world
                .objects
                .iter()
                .filter(|o| o.owner != me.owner && !o.limbo && o.health > 0)
                .min_by_key(|o| (i64::from(distance(o.coord, me.coord)), o.id))
        })
        .map(|o| (o.id, o.coord));
    let Some((target_id, target_at)) = target else {
        return;
    };
    let fresh = me.motion.target != Some(target_id);
    let infantry = me.kind == Kind::Infantry;
    let o = &mut world.objects[i];
    o.motion.target = Some(target_id);
    o.motion.reload = (o.motion.reload - 1).max(0);
    let in_range = |c: Coord| distance(c, target_at) <= weapon.range;

    // Both drive or walk cell to cell along the route toward the target,
    // stopping in the first cell with the target in range. Vehicles stand
    // on cell centres; infantry on a sub-cell spot of each cell (spot 2 on
    // the one route observed). `[hypothesis]`
    let stand = |cell: Coord| {
        if infantry {
            Coord {
                x: cell.x + SPOTS[2].0 - 128,
                y: cell.y + SPOTS[2].1 - 128,
                z: cell.z,
            }
        } else {
            cell
        }
    };
    // Infantry set off the frame after picking a target; vehicles turn
    // that frame.
    if !(o.motion.arrived || infantry && fresh) {
        if o.motion.leg.is_none() {
            let cell = cell_centre(o.coord);
            if o.motion.rolling && in_range(cell) || !o.motion.rolling && in_range(o.coord) {
                o.motion.arrived = true;
                o.motion.arrived_at = frame;
            } else {
                // Plan once, when there's no route left: toward the cell
                // the weapon reaches from, through the game's planner.
                let here = to_cell(cell);
                if o.motion.route.is_empty() {
                    // Infantry close to their full range; vehicles to half
                    // a cell inside it. `[observed]` (the GI walks to
                    // (37,38) where range less half a cell gives (37,37))
                    let reach = weapon.range + if infantry { 128 } else { 0 };
                    let goal = pathfind::approach(here, to_cell(target_at), reach);
                    o.motion.goal = Some(Coord {
                        x: i32::from(goal.0) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
                        y: i32::from(goal.1) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
                        z: cell.z,
                    });
                    let cells = &world.cells;
                    let valid = |c: (i16, i16)| {
                        if cells.is_empty() {
                            (c.0 - here.0).abs() <= 32 && (c.1 - here.1).abs() <= 32
                        } else {
                            cells.contains(&c)
                        }
                    };
                    o.motion.route = pathfind::plan(here, goal, &valid).unwrap_or_default();
                    o.motion.route.reverse();
                }
                let next = match o.motion.route.pop() {
                    Some(d) => {
                        let (dx, dy) = pathfind::DIRECTIONS[usize::from(d)];
                        Coord {
                            x: cell.x + i32::from(dx) * LEPTONS_PER_CELL,
                            y: cell.y + i32::from(dy) * LEPTONS_PER_CELL,
                            z: cell.z,
                        }
                    }
                    // Nowhere planned to go: stand and wait in range.
                    None => cell,
                };
                if next == cell {
                    o.motion.arrived = true;
                    o.motion.arrived_at = frame;
                } else {
                    o.motion.leg = Some((o.coord, stand(next), 0));
                }
            }
        }
        if let Some((from, to, _)) = o.motion.leg {
            let want = step_facing(to.x - from.x, to.y - from.y);
            let facing = i32::from(o.dir);
            if infantry || o.motion.rolling || facing == want {
                o.dir = u16::try_from(want).unwrap_or(0);
            } else {
                o.dir = u16::try_from(turn(facing, want, t.rot * 256)).unwrap_or(0);
            }
            if i32::from(o.dir) == want {
                o.motion.rolling = true;
                let speed = if infantry {
                    object_speed(rules, t, o) * WALK_SPEED
                } else {
                    let dist = o.motion.goal.map_or(i32::MAX, |g| distance(g, o.coord));
                    throttle(rules, o, t, dist)
                };
                travel(o, speed);
            }
        }
    }
    // A turret turns toward the target at the body's `ROT=` and fires
    // only on it. `[observed]` (`allied-depot-repair`: a Grizzly's turret,
    // 135° off, fired 20 frames after the order: 24,576 ÷ 1,280, rounded
    // up)
    let aimed = if t.turret {
        let want = facing_to(o.coord, target_at);
        let now = turn(i32::from(o.turret.unwrap_or(o.dir)), want, t.rot * 256);
        o.turret = Some(u16::try_from(now).unwrap_or(0));
        now == want
    } else {
        true
    };
    // Turreted vehicles fire whenever the target is in range, moving or
    // not; infantry only standing, from the frame after they stop.
    let ready = if infantry {
        o.motion.arrived && frame > o.motion.arrived_at
    } else {
        in_range(o.coord) && aimed
    };
    if ready && o.motion.reload == 0 {
        fire(world, rules, rng, frame, i, target_id, infantry);
    }
}

fn fire(
    world: &mut World,
    rules: &Rules,
    rng: &mut Randomizer,
    frame: u32,
    i: usize,
    target: u32,
    infantry: bool,
) {
    let o = &world.objects[i];
    let t = &rules.technos[o.techno];
    let rank = o.rank();
    let Some(w) = weapon_now(world, rules, o) else {
        return;
    };
    let weapon = rules.weapon(w);
    let Some(target_at) = world
        .objects
        .iter()
        .find(|x| x.id == target)
        .map(|x| x.coord)
    else {
        return;
    };
    let arcing = weapon
        .projectile
        .is_some_and(|p| rules.projectile(p).arcing);
    let mind = weapon
        .warhead
        .is_some_and(|wh| rules.warhead(wh).mind_control);
    let flight = if arcing {
        distance(o.coord, target_at) / ARC_LEPTONS_PER_FRAME
    } else if mind {
        MIND_CONTROL_FLIGHT
    } else {
        0
    };
    let shooter = o.id;
    let damage =
        ra2_rules::veterancy::firepower(rules, t, rank, o.firepower_bonus.of(weapon.damage));
    world.shots.push(Shot {
        shooter,
        damage,
        target,
        weapon: w,
        lands: frame + 1 + u32::try_from(flight).unwrap_or(0),
    });
    // The reload. Between the shots of a burst, 3 to 5 frames; after the
    // last, ROF plus 0 to 2, then × `VeteranROF=` if it has `ROF`, and
    // infantry a little more. `[recomp 0x006FD084, 0x006FD0A6]` (intervals
    // 60–62 for ROF 60, 64–67 for 65, 22–25 for 20 `[observed]`; the
    // infantry part is fitted)
    let o = &mut world.objects[i];
    o.motion.burst += 1;
    o.motion.reload = if o.motion.burst < weapon.burst {
        rng.ranged(3, 5)
    } else {
        o.motion.burst = 0;
        ra2_rules::veterancy::rof(rules, t, rank, weapon.rof + rng.ranged(0, 2))
            + if infantry { 2 } else { 0 }
    };
}

/// The weapon a unit fires: an elite one's `ElitePrimary=`, if it has one.
/// `[community]`
fn weapon_of(t: &ra2_rules::TechnoType, rank: u8) -> Option<ra2_rules::WeaponId> {
    if rank >= 2 {
        t.elite_primary.or(t.primary)
    } else {
        t.primary
    }
}

/// An object's weapon: a gunner's (`Gunner=yes`, the IFV) is its
/// `Weapon<n>`, n its first passenger's `IFVMode=` + 1, the first when
/// empty. `[community]` (`[observed]`: an engineer aboard an IFV mended)
pub(crate) fn weapon_now(world: &World, rules: &Rules, o: &Object) -> Option<ra2_rules::WeaponId> {
    let t = &rules.technos[o.techno];
    if t.gunner && !t.weapons.is_empty() {
        let mode = o
            .passengers
            .first()
            .and_then(|id| world.objects.iter().find(|p| p.id == *id))
            .map_or(0, |p| rules.technos[p.techno].ifv_mode);
        let list = if o.rank() >= 2 {
            &t.elite_weapons
        } else {
            &t.weapons
        };
        let k = usize::try_from(mode).unwrap_or(0);
        return list
            .get(k)
            .copied()
            .flatten()
            .or(t.weapons.first().copied().flatten());
    }
    weapon_of(t, o.rank())
}

/// Apply the damage of every shell landing this frame; remove the dead.
fn land(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32) {
    let (now, later): (Vec<Shot>, Vec<Shot>) =
        world.shots.drain(..).partition(|s| s.lands <= frame);
    world.shots = later;
    for s in now {
        let weapon = rules.weapon(s.weapon);
        let Some(o) = world.objects.iter_mut().find(|o| o.id == s.target) else {
            continue;
        };
        // A building under the Force Shield takes nothing. `[hypothesis]`
        if o.shielded_until > frame || o.health <= 0 {
            continue;
        }
        if weapon
            .warhead
            .is_some_and(|wh| rules.warhead(wh).mind_control)
        {
            let target = o.id;
            control(world, s.shooter, target, weapon.damage);
            continue;
        }
        // Its veterancy's `STRONGER` first, then the warhead's `Verses=`.
        // `[recomp 0x007019CB]`
        let ot = &rules.technos[o.techno];
        let raw = ra2_rules::veterancy::armor(rules, ot, o.rank(), s.damage);
        // A crate's armor bonus divides it too. `[hypothesis]`
        let raw = if o.armor_bonus == ra2_rules::Ratio::ONE {
            raw
        } else {
            (ra2_rules::x87::X80::int(raw) / ra2_rules::x87::X80::double(o.armor_bonus.value()))
                .trunc()
                .max(1)
        };
        let damage = weapon
            .warhead
            .map_or(raw, |wh| rules.warhead(wh).verses[ot.armor.index()].of(raw));
        // A mending weapon's negative damage heals, up to full strength.
        // `[hypothesis]`
        o.health = (o.health - damage).min(if damage < 0 { ot.strength } else { i32::MAX });
        if o.health <= 0 {
            let (victim, rank, owner, id) = (o.techno, o.rank(), o.owner, o.id);
            credit(world, rules, s.shooter, victim, rank, owner);
            if let Some(k) = world.objects.iter().position(|o| o.id == id) {
                crate::death::destroyed(world, rules, rng, k, frame);
            }
        }
    }
    world
        .objects
        .retain(|o| o.health > 0 || o.dies_at.is_some());
}

/// A mind control link: the target becomes the shooter's house's, unless
/// it's a building or already its own; past the weapon's `Damage=` links
/// (Yuri's 1), the oldest is let go, back to its house, on the same frame.
/// `[observed]` (`yuri-mind-control`: a Neutral Grizzly became the Yuri
/// player's, and went back when Yuri took a Rhino)
fn control(world: &mut World, shooter: u32, target: u32, links: i32) {
    let Some(owner) = world
        .objects
        .iter()
        .find(|o| o.id == shooter)
        .map(|o| o.owner)
    else {
        return;
    };
    let Some(t) = world.objects.iter().position(|o| o.id == target) else {
        return;
    };
    if world.objects[t].kind == Kind::Building || world.objects[t].owner == owner {
        return;
    }
    let mut held: Vec<usize> = (0..world.objects.len())
        .filter(|&k| {
            world.objects[k]
                .controlled_by
                .is_some_and(|(c, _)| c == shooter)
        })
        .collect();
    while i32::try_from(held.len()).unwrap_or(i32::MAX) >= links.max(1) {
        let k = held.remove(0);
        if let Some((_, was)) = world.objects[k].controlled_by.take() {
            world.objects[k].owner = was;
        }
    }
    let o = &mut world.objects[t];
    // It keeps its mission: a sleeping Grizzly taken stayed asleep.
    // `[observed]`
    o.controlled_by = Some((shooter, o.owner));
    o.owner = owner;
    o.command = None;
    o.motion.target = None;
}

/// Units a dead controller held go back to their houses. `[hypothesis]`
pub(crate) fn release(world: &mut World) {
    let alive: Vec<u32> = world
        .objects
        .iter()
        .filter(|o| o.health > 0)
        .map(|o| o.id)
        .collect();
    for o in &mut world.objects {
        if let Some((c, was)) = o.controlled_by
            && !alive.contains(&c)
        {
            o.owner = was;
            o.controlled_by = None;
        }
    }
}

/// A kill's experience for its shooter, if it's trainable and the victim
/// isn't its own side's (see `production::experience_after_kill`).
/// `[recomp 0x00702D50]`
fn credit(world: &mut World, rules: &Rules, shooter: u32, victim: usize, rank: u8, owner: u32) {
    let Some(k) = world.objects.iter().position(|x| x.id == shooter) else {
        return;
    };
    let t = &rules.technos[world.objects[k].techno];
    if !t.trainable || world.objects[k].owner == owner {
        return;
    }
    let worth = rules.technos[victim].cost * (i32::from(rank) + 1);
    world.objects[k].experience_bits = ra2_rules::production::experience_after_kill(
        rules,
        world.objects[k].experience_bits,
        worth,
        t.cost,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facings_toward_points() {
        let c = |x, y| Coord { x, y, z: 0 };
        let o = c(1000, 1000);
        assert_eq!(facing_to(o, c(1000, 0)), 0);
        assert_eq!(facing_to(o, c(2000, 0)), 8192);
        assert_eq!(facing_to(o, c(2000, 1000)), 16_384);
        assert_eq!(facing_to(o, c(1000, 2000)), 32_768);
        assert_eq!(facing_to(o, c(0, 1000)), 49_152);
        assert_eq!(facing_to(o, c(0, 0)), 57_344);
        let a = facing_to(o, c(1500, 0));
        assert!((a - 4836).abs() <= 2, "atan(1/2): {a}");
    }

    #[test]
    fn accelerating_drivers_crawl_off_speed_up_and_slow_near_the_end() {
        let rules = ra2_rules::Rules::from_ini(
            &ra2_rules::Ini::parse(b"[VehicleTypes]\n1=HARV\n2=MTNK\n[HARV]\nSpeed=4\n[MTNK]\nSpeed=7\nAccelerates=no\n"),
            &ra2_rules::Ini::parse(b""),
        );
        let (harv, mtnk) = (&rules.technos[0], &rules.technos[1]);
        let mut o = crate::world::World::default();
        o.add_house("Neutral", 0);
        let p = ra2_rules::scenario::Placement {
            kind: ra2_rules::Kind::Vehicle,
            type_id: "HARV".into(),
            owner: "Neutral".into(),
            cell: (1, 1),
            sub_cell: 0,
            health: 256,
            facing: 0,
            mission: "Guard".into(),
            veterancy: 0,
        };
        o.place(&rules, &p, 0);
        let unit = &mut o.objects[0];
        let far = 10_000;
        // Points a frame: 3 at the crawl, one more about every 3 frames.
        let points: Vec<i32> = (0..25)
            .map(|_| throttle(&rules, unit, harv, far) / DRIVE_SPEED)
            .collect();
        assert_eq!(&points[..5], &[3, 3, 3, 3, 4]);
        assert_eq!(points[24], 10, "full speed after about 23 frames");
        // Within 500 leptons it loses 0.02 a frame, down to the crawl.
        let slowing: Vec<i32> = (0..40)
            .map(|_| throttle(&rules, unit, harv, 300) / DRIVE_SPEED)
            .collect();
        assert_eq!(slowing[0], 9);
        assert_eq!(*slowing.last().expect("some"), 3);
        // Units that don't accelerate go at full speed at once.
        let mut tank = o.objects[0].clone();
        tank.motion = Motion::default();
        assert_eq!(throttle(&rules, &mut tank, mtnk, far), 17 * DRIVE_SPEED);
    }

    #[test]
    fn turning_goes_the_short_way_at_the_turn_rate() {
        assert_eq!(turn(16_384, 57_344, 1280), 15_104);
        assert_eq!(turn(57_344, 0, 1280), 58_624);
        assert_eq!(turn(1000, 0, 1280), 0);
    }

    #[test]
    fn steps_face_their_direction() {
        assert_eq!(step_facing(0, -1), 0);
        assert_eq!(step_facing(-1, -1), 57_344);
        assert_eq!(step_facing(1, 0), 16_384);
    }
}
