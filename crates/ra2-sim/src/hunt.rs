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
const DRIVE_SPEED: i32 = 406;
const WALK_SPEED: i32 = 277;
/// Leptons of distance per frame of an arcing shell's flight. `[observed]`
const ARC_LEPTONS_PER_FRAME: i32 = 128;
/// A full turn in the 16-bit facing the game keeps.
const FULL_TURN: i32 = 65_536;

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
fn step_facing(dx: i32, dy: i32) -> i32 {
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

/// Turn `from` toward `to` by at most `rate`, the short way round.
fn turn(from: i32, to: i32, rate: i32) -> i32 {
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

fn to_cell(c: Coord) -> (i16, i16) {
    (
        i16::try_from(c.x.div_euclid(LEPTONS_PER_CELL)).unwrap_or(0),
        i16::try_from(c.y.div_euclid(LEPTONS_PER_CELL)).unwrap_or(0),
    )
}

fn cell_centre(c: Coord) -> Coord {
    Coord {
        x: c.x.div_euclid(LEPTONS_PER_CELL) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        y: c.y.div_euclid(LEPTONS_PER_CELL) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        z: c.z,
    }
}

/// Advance along the current leg; returns whether the leg finished.
fn travel(o: &mut Object, speed_256: i32) -> bool {
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
        if world.objects[i].mission != Mission::Hunt {
            continue;
        }
        hunt(world, rules, rng, frame, i);
    }
    land(world, rules, frame);
}

#[allow(clippy::too_many_lines)]
fn hunt(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32, i: usize) {
    let me = world.objects[i].clone();
    let t = &rules.technos[me.techno];
    let Some(weapon) = t.primary.map(|w| rules.weapon(w)) else {
        return;
    };
    // The target: kept while it lives, else the nearest object of another
    // house. `[hypothesis]` (threat and house alliances not yet weighed)
    let target = me
        .motion
        .target
        .and_then(|id| world.objects.iter().find(|o| o.id == id))
        .or_else(|| {
            world
                .objects
                .iter()
                .filter(|o| o.owner != me.owner)
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
                let speed = if infantry { WALK_SPEED } else { DRIVE_SPEED };
                travel(o, t.speed_fraction * speed);
            }
        }
    }
    // Turreted vehicles fire whenever the target is in range, moving or
    // not; infantry only standing, from the frame after they stop.
    let ready = if infantry {
        o.motion.arrived && frame > o.motion.arrived_at
    } else {
        in_range(o.coord)
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
    let Some(w) = t.primary else { return };
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
    let flight = if arcing {
        distance(o.coord, target_at) / ARC_LEPTONS_PER_FRAME
    } else {
        0
    };
    world.shots.push(Shot {
        target,
        weapon: w,
        lands: frame + 1 + u32::try_from(flight).unwrap_or(0),
    });
    // The reload: ROF plus a small random part, and infantry a little
    // more. `[observed]` (intervals 60–62 for ROF 60, 64–67 for 65, 22–25
    // for 20)
    let extra = rng.ranged(0, 2) + if infantry { 2 } else { 0 };
    world.objects[i].motion.reload = weapon.rof + extra;
}

/// Apply the damage of every shell landing this frame; remove the dead.
fn land(world: &mut World, rules: &Rules, frame: u32) {
    let (now, later): (Vec<Shot>, Vec<Shot>) =
        world.shots.drain(..).partition(|s| s.lands <= frame);
    world.shots = later;
    for s in now {
        let weapon = rules.weapon(s.weapon);
        let Some(o) = world.objects.iter_mut().find(|o| o.id == s.target) else {
            continue;
        };
        let armor = rules.technos[o.techno].armor;
        let damage = weapon.warhead.map_or(weapon.damage, |wh| {
            rules.warhead(wh).verses[armor.index()].of(weapon.damage)
        });
        o.health -= damage;
    }
    world.objects.retain(|o| o.health > 0);
}

#[cfg(test)]
mod tests {
    use super::*;

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
