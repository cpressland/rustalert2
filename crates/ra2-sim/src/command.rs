//! Orders a player gives a unit: move to a cell, attack, enter, capture.
//! See `docs/spec/locomotors.md` (moving) and `docs/spec/repair.md`
//! (entering a Service Depot).

use ra2_rules::Rules;

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::hunt::{cell_centre, distance, step_facing, throttle, to_cell, travel, turn};
use crate::pathfind;
use crate::world::{Mission, World};

/// What a unit was told to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Drive to a cell and stop.
    Move((i16, i16)),
    /// Attack an object (by id) until it's gone, as a hunter does.
    Attack(u32),
    /// Go into an object (by id): a Service Depot, a transport.
    Enter(u32),
    /// An engineer's order on a building (by id): capture an enemy's,
    /// repair its own side's.
    Capture(u32),
}

/// A unit on a Service Depot: when it arrived, when its next repair step
/// is due, and when it was whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DepotVisit {
    pub next: u32,
    pub steps: u32,
    pub whole_at: Option<u32>,
}

/// After docking, the first repair step comes this many frames on, the
/// second this many after it, and then one every
/// `production::unit_repair_frames`. The long second wait is likely the
/// depot's animation. `[observed]` (`allied-depot-repair`)
const DEPOT_FIRST_STEP: u32 = 4;
const DEPOT_SECOND_STEP: u32 = 85;
/// A repaired unit drives off this long after it's whole. `[observed]`
const DEPOT_LEAVE: u32 = 12;

/// Give the unit with `id` a command, from the frame it lands.
pub fn order(world: &mut World, id: u32, command: Command) {
    let Some(o) = world.objects.iter_mut().find(|o| o.id == id) else {
        return;
    };
    o.command = Some(command);
    o.mission = Mission::Other;
    o.motion.route.clear();
    o.motion.planned = None;
    o.motion.target = match command {
        Command::Attack(t) => Some(t),
        _ => None,
    };
}

/// One frame of every commanded unit.
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    let mut gone = Vec::new();
    for i in 0..world.objects.len() {
        let command = world.objects[i].command;
        match command {
            Some(Command::Move(goal)) => {
                if drive(world, rules, i, goal) {
                    let o = &mut world.objects[i];
                    o.command = None;
                    o.mission = Mission::Guard;
                }
            }
            Some(Command::Capture(id)) if capture(world, rules, i, id) => {
                gone.push(world.objects[i].id);
            }
            Some(Command::Enter(id)) => enter(world, rules, frame, i, id),
            _ => {}
        }
    }
    world.objects.retain(|o| !gone.contains(&o.id));
}

/// A unit sent into a building. A Service Depot (`UnitRepair=yes`): it
/// drives onto the depot's centre cell and is repaired there,
/// `RepairStep=` hit points a step for what that costs (see
/// `production::repair_step_cost`), then drives off the pad to the cell
/// below the depot's left column. `[observed]` (`allied-depot-repair`: an
/// IFV, 3 credits a step) What happens when the house can't pay isn't
/// recorded: it leaves.
fn enter(world: &mut World, rules: &Rules, frame: u32, i: usize, id: u32) {
    let Some(b) = world.objects.iter().position(|o| o.id == id && !o.limbo) else {
        stand(world, i);
        return;
    };
    let depot = &rules.technos[world.objects[b].techno];
    if !depot.unit_repair {
        stand(world, i);
        return;
    }
    let (x0, y0) = crate::production::top_left(rules, &world.objects[b]);
    let (w, h) = depot.foundation;
    let pad = (cell_i16(x0 + w / 2), cell_i16(y0 + h / 2));
    if let Some(visit) = world.objects[i].depot {
        let t = &rules.technos[world.objects[i].techno];
        if let Some(at) = visit.whole_at {
            if frame >= at + DEPOT_LEAVE {
                let o = &mut world.objects[i];
                o.depot = None;
                o.command = Some(Command::Move((cell_i16(x0), cell_i16(y0 + h))));
            }
            return;
        }
        if frame < visit.next {
            return;
        }
        let cost = ra2_rules::production::repair_step_cost(rules, t);
        let owner = usize::try_from(world.objects[i].owner).unwrap_or(usize::MAX);
        let Some(house) = world.houses.get_mut(owner) else {
            return;
        };
        let strength = t.strength;
        let o = &mut world.objects[i];
        if house.credits < cost {
            o.depot = Some(DepotVisit {
                whole_at: Some(frame),
                ..visit
            });
            return;
        }
        house.credits -= cost;
        o.health = (o.health + rules.general.repair_step).min(strength);
        let every = u32::try_from(ra2_rules::production::unit_repair_frames(rules)).unwrap_or(1);
        o.depot = Some(DepotVisit {
            next: frame
                + if visit.steps == 0 {
                    DEPOT_SECOND_STEP
                } else {
                    every
                },
            steps: visit.steps + 1,
            whole_at: (o.health >= strength).then_some(frame),
        });
        return;
    }
    if drive(world, rules, i, pad) {
        world.objects[i].depot = Some(DepotVisit {
            next: frame + DEPOT_FIRST_STEP,
            steps: 0,
            whole_at: None,
        });
    }
}

fn stand(world: &mut World, i: usize) {
    let o = &mut world.objects[i];
    o.command = None;
    o.mission = Mission::Guard;
}

fn cell_i16(v: i32) -> i16 {
    i16::try_from(v).unwrap_or(0)
}

/// An engineer on its way to a building: it walks to the building's
/// nearest cell and goes in when it reaches one. Its own side's building
/// is repaired to full strength, at no cost, and the engineer is used up.
/// `[observed]` (`yuri-engineer-repair`: a Bio Reactor at 500 of 700, whole
/// on the frame the engineer reached its top-left cell) True once it's
/// gone in. Capturing an enemy's building isn't recorded yet.
fn capture(world: &mut World, rules: &Rules, i: usize, id: u32) -> bool {
    let Some(b) = world.objects.iter().position(|o| o.id == id && !o.limbo) else {
        let o = &mut world.objects[i];
        o.command = None;
        o.mission = Mission::Guard;
        return false;
    };
    let (x0, y0) = crate::production::top_left(rules, &world.objects[b]);
    let (w, h) = rules.technos[world.objects[b].techno].foundation;
    let here = to_cell(world.objects[i].coord);
    let inside = |c: (i16, i16)| {
        (x0..x0 + w).contains(&i32::from(c.0)) && (y0..y0 + h).contains(&i32::from(c.1))
    };
    if inside(here) {
        let same_side = world.objects[b].owner == world.objects[i].owner;
        if same_side {
            let strength = rules.technos[world.objects[b].techno].strength;
            world.objects[b].health = strength;
        }
        return true;
    }
    // The nearest of its cells, by distance from the engineer's cell.
    let mut best: Option<((i16, i16), i64)> = None;
    for y in y0..y0 + h {
        for x in x0..x0 + w {
            let (Ok(cx), Ok(cy)) = (i16::try_from(x), i16::try_from(y)) else {
                continue;
            };
            let (dx, dy) = (i64::from(cx - here.0), i64::from(cy - here.1));
            let d = dx * dx + dy * dy;
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some(((cx, cy), d));
            }
        }
    }
    let Some((goal, _)) = best else {
        return false;
    };
    drive(world, rules, i, goal);
    false
}

/// Drive toward `goal` along a planned route: turn on the spot to the
/// first step, then cell centre to cell centre, as harvesters do. True
/// once there.
pub(crate) fn drive(world: &mut World, rules: &Rules, i: usize, goal: (i16, i16)) -> bool {
    let t = &rules.technos[world.objects[i].techno];
    let cells = &world.cells;
    let o = &mut world.objects[i];
    if o.motion.planned != Some(goal) {
        o.motion.planned = Some(goal);
        let here = to_cell(cell_centre(o.coord));
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
    if o.motion.leg.is_none() {
        let cell = cell_centre(o.coord);
        let Some(d) = o.motion.route.pop() else {
            o.motion.planned = None;
            o.motion.rolling = false;
            return true;
        };
        let (dx, dy) = pathfind::DIRECTIONS[usize::from(d)];
        let next = Coord {
            x: cell.x + i32::from(dx) * LEPTONS_PER_CELL,
            y: cell.y + i32::from(dy) * LEPTONS_PER_CELL,
            z: cell.z,
        };
        o.motion.leg = Some((o.coord, next, 0));
    }
    if let Some((from, to, _)) = o.motion.leg {
        let want = step_facing(to.x - from.x, to.y - from.y);
        let facing = i32::from(o.dir);
        if o.kind == ra2_rules::Kind::Infantry {
            // Infantry turn at once and walk. `[observed]` (as hunting)
            o.dir = u16::try_from(want).unwrap_or(0);
            let speed = crate::hunt::top_speed(rules, t, o.rank()) * crate::hunt::WALK_SPEED;
            if travel(o, speed) && to_cell(o.coord) == goal {
                o.motion.planned = None;
                o.motion.route.clear();
                return true;
            }
            return false;
        }
        if o.motion.rolling {
            o.dir = u16::try_from(want).unwrap_or(0);
        } else {
            o.dir = u16::try_from(turn(facing, want, t.rot * 256)).unwrap_or(0);
        }
        if i32::from(o.dir) == want {
            let to_goal = Coord {
                x: i32::from(goal.0) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
                y: i32::from(goal.1) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
                z: o.coord.z,
            };
            let speed = throttle(rules, o, t, distance(to_goal, o.coord));
            o.motion.rolling = true;
            if travel(o, speed) && to_cell(o.coord) == goal {
                o.motion.planned = None;
                o.motion.rolling = false;
                o.motion.route.clear();
                return true;
            }
        }
    }
    false
}
