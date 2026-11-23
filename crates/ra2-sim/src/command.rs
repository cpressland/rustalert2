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
}

/// Give the unit with `id` a command, from the frame it lands.
pub fn order(world: &mut World, id: u32, command: Command) {
    let Some(o) = world.objects.iter_mut().find(|o| o.id == id) else {
        return;
    };
    o.command = Some(command);
    o.mission = Mission::Other;
    o.motion.route.clear();
    o.motion.planned = None;
}

/// One frame of every commanded unit.
pub fn update(world: &mut World, rules: &Rules, _frame: u32) {
    for i in 0..world.objects.len() {
        let Some(Command::Move(goal)) = world.objects[i].command else {
            continue;
        };
        if drive(world, rules, i, goal) {
            let o = &mut world.objects[i];
            o.command = None;
            o.mission = Mission::Guard;
        }
    }
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
