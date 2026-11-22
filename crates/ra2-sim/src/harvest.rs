//! The Harvest mission: find ore, take it a bail at a time, carry a full
//! load to a refinery, and go back for more. See `docs/spec/economy.md`.
//!
//! The mission's logic is traced from the game (`UnitClass::Mission_Harvest`
//! at `0x0073E5E0`); docking is fitted to the parity reference and marked so.

use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::hunt::{cell_centre, distance, step_facing, throttle, to_cell, travel, turn};
use crate::pathfind;
use crate::rng::Randomizer;
use crate::world::{Mission, World};

/// Frames from one bail to the next: the harvest stage counts up every
/// `HarvesterLoadRate` frames (default 2), the mission takes a bail once it
/// reaches 9, and that is the 19th frame. `[recomp 0x0073E931]`
const BAIL_FRAMES: u32 = 19;
/// The mission's usual delay between looks: `[Harvest] Rate=` (.016
/// minutes) × 900, truncated, plus a random 0–2. `[recomp 0x0073EF77]`
const RATE_FRAMES: i32 = 14;
/// The first look, frames into the game. `[observed]`
const FIRST_LOOK: u32 = 9;
/// Frames between asking a refinery to dock and setting off for it.
/// `[observed]`
const DOCK_REPLY: u32 = 3;
/// How fast a harvester backs onto the pad, in 1/256 leptons a frame.
/// `[observed]`
const DOCK_SPEED: i32 = 1150;
/// Frames on the pad before the load is paid for, and then before leaving.
/// `[observed]`
const UNLOAD_FRAMES: u32 = 40;
const LEAVE_FRAMES: u32 = 31;

/// Where a harvester is in its round, as the game's mission status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum State {
    /// Looking for ore, or driving to it.
    #[default]
    Look,
    /// Taking bails, or moving to the next ore cell.
    Gather,
    /// Full: off to a refinery.
    FindHome,
    /// Docking, unloading, leaving.
    Dock,
    /// Nothing to do.
    Idle,
}

/// The steps of a visit to a refinery. `[observed]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visit {
    /// Waiting for the refinery's reply, until the frame given.
    Asked(u32),
    /// Driving to the refinery's queueing cell.
    Approach,
    /// Turning round to back onto the pad.
    TurnRound,
    /// Backing onto the pad.
    Reverse,
    /// On the pad since the frame given.
    Unload(u32),
    /// Paid; leaving at the frame given.
    Paid(u32),
}

/// A harvester's state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Harvest {
    pub state: State,
    /// The frame of the mission's next look; 0 until the first.
    pub next_look: u32,
    /// The frame the harvest stage started counting, if it is.
    pub stage_from: Option<u32>,
    /// The cell it is driving to.
    pub dest: Option<(i16, i16)>,
    /// The cell `route` was planned to.
    pub planned: Option<(i16, i16)>,
    /// Where to come back to after unloading.
    pub archive: Option<(i16, i16)>,
    /// The refinery it's visiting, and how far along.
    pub visit: Option<(u32, Visit)>,
}

/// One frame of every harvester on the Harvest mission: its mission when
/// due, then its movement.
pub fn update(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32) {
    for i in 0..world.objects.len() {
        let o = &world.objects[i];
        if o.mission != Mission::Harvest || !rules.technos[o.techno].harvester {
            continue;
        }
        if o.harvest.next_look == 0 {
            world.objects[i].harvest.next_look = FIRST_LOOK;
        }
        if frame >= world.objects[i].harvest.next_look {
            let delay = mission(world, rules, rng, frame, i);
            world.objects[i].harvest.next_look = frame + u32::try_from(delay.max(1)).unwrap_or(1);
        }
        visit(world, rules, frame, i);
        drive(world, rules, i);
    }
}

/// The game's usual mission delay.
fn rate(rng: &mut Randomizer) -> i32 {
    RATE_FRAMES + rng.ranged(0, 2)
}

/// The harvest mission, once; returns frames until it runs again.
fn mission(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32, i: usize) -> i32 {
    let t = &rules.technos[world.objects[i].techno];
    let full = world.objects[i].cargo >= t.storage;
    let short = rules.general.tiberium_short_scan;
    let long = rules.general.tiberium_long_scan;
    match world.objects[i].harvest.state {
        State::Look => {
            if full {
                world.objects[i].harvest.state = State::FindHome;
                return 1;
            }
            if goto_ore(world, rules, i, long) {
                let h = &mut world.objects[i].harvest;
                h.stage_from = Some(frame);
                h.state = State::Gather;
                return 1;
            }
            if world.objects[i].harvest.dest.is_some() {
                return rate(rng);
            }
            world.objects[i].harvest.state = State::Idle;
            105
        }
        State::Gather => {
            let h = &mut world.objects[i].harvest;
            let Some(from) = h.stage_from else {
                h.stage_from = Some(frame);
                return 1;
            };
            if frame < from + BAIL_FRAMES {
                return 1;
            }
            if take_bail(world, rules, frame, i) {
                return 1;
            }
            if world.objects[i].cargo >= t.storage {
                // Full: remember the ore nearest here to come back to.
                let back = find_ore(world, rules, i, short);
                let h = &mut world.objects[i].harvest;
                h.archive = back;
                h.state = State::FindHome;
                return 1;
            }
            if goto_ore(world, rules, i, short) || world.objects[i].harvest.dest.is_some() {
                return 1;
            }
            let h = &mut world.objects[i].harvest;
            h.archive = None;
            h.state = State::FindHome;
            1
        }
        State::FindHome => {
            let me = &world.objects[i];
            let home = world
                .objects
                .iter()
                .filter(|b| {
                    b.kind == Kind::Building
                        && b.owner == me.owner
                        && t.dock
                            .iter()
                            .any(|d| d.eq_ignore_ascii_case(&rules.technos[b.techno].id))
                })
                .min_by_key(|b| (distance(b.coord, me.coord), b.id))
                .map(|b| b.id);
            let h = &mut world.objects[i].harvest;
            match home {
                Some(id) => {
                    h.visit = Some((id, Visit::Asked(frame + DOCK_REPLY)));
                    h.state = State::Dock;
                }
                None => h.state = State::Idle,
            }
            rate(rng)
        }
        State::Dock | State::Idle => rate(rng),
    }
}

/// `FootClass::Goto_Tiberium`: true if it stands on the ore it would go
/// to; otherwise sets off for it, unless it's already going somewhere.
/// `[recomp 0x004DCFE0]`
fn goto_ore(world: &mut World, rules: &Rules, i: usize, radius: i32) -> bool {
    if world.objects[i].harvest.dest.is_some() {
        return false;
    }
    let Some(cell) = find_ore(world, rules, i, radius) else {
        return false;
    };
    if cell == to_cell(world.objects[i].coord) {
        return true;
    }
    world.objects[i].harvest.dest = Some(cell);
    false
}

/// `FootClass::Search_For_Tiberium`: its own cell if that has ore, else
/// the best ore in the nearest ring of cells round it that has any, out to
/// `radius` cells. Each ring is scanned edge by edge — for each step `i`
/// along it, the top, bottom, left and right edges' cells — and the first
/// cell of the highest worth wins. `[recomp 0x004DD0A0]`
pub fn find_ore(world: &World, rules: &Rules, i: usize, radius: i32) -> Option<(i16, i16)> {
    let me = &world.objects[i];
    let here = to_cell(me.coord);
    if world.ore.contains_key(&here) {
        return Some(here);
    }
    let (hx, hy) = (i32::from(here.0), i32::from(here.1));
    let mut best: Option<((i16, i16), i32)> = None;
    for r in 1..radius {
        for step in -r..=r {
            for (dx, dy) in [(step, -r), (step, r), (-r, step), (r, step)] {
                let (Ok(x), Ok(y)) = (i16::try_from(hx + dx), i16::try_from(hy + dy)) else {
                    continue;
                };
                let cell = (x, y);
                let Some(ore) = world.ore.get(&cell) else {
                    continue;
                };
                if !(world.cells.is_empty() || world.cells.contains(&cell))
                    || occupied(world, rules, cell, me.id)
                {
                    continue;
                }
                let worth = ore.worth(rules);
                if best.is_none_or(|(_, w)| worth > w) {
                    best = Some((cell, worth));
                }
            }
        }
        if best.is_some() {
            return best.map(|(c, _)| c);
        }
    }
    None
}

/// Whether another object stands on a cell (a building on any cell of its
/// foundation).
fn occupied(world: &World, rules: &Rules, cell: (i16, i16), me: u32) -> bool {
    world.objects.iter().any(|o| {
        if o.id == me {
            return false;
        }
        if o.kind == Kind::Building {
            let (w, h) = rules.technos[o.techno].foundation;
            let x0 = o.coord.x - w * LEPTONS_PER_CELL / 2;
            let y0 = o.coord.y - h * LEPTONS_PER_CELL / 2;
            let (cx, cy) = (
                i32::from(cell.0) * LEPTONS_PER_CELL,
                i32::from(cell.1) * LEPTONS_PER_CELL,
            );
            (x0..x0 + w * LEPTONS_PER_CELL).contains(&cx)
                && (y0..y0 + h * LEPTONS_PER_CELL).contains(&cy)
        } else {
            to_cell(o.coord) == cell
        }
    })
}

/// `UnitClass::Harvesting`: take a bail from the cell it stands on. True
/// while it's busy (still driving, or it took one). A cell whose data is
/// down to 0 gives no bail: its overlay goes. `[recomp 0x0073D450]`
fn take_bail(world: &mut World, rules: &Rules, frame: u32, i: usize) -> bool {
    let o = &world.objects[i];
    if o.harvest.dest.is_some() {
        return true;
    }
    if o.cargo >= rules.technos[o.techno].storage {
        return false;
    }
    let here = to_cell(o.coord);
    let Some(ore) = world.ore.get(&here).copied() else {
        world.objects[i].harvest.stage_from = None;
        return false;
    };
    if ore.data == 0 {
        world.ore.remove(&here);
        return false;
    }
    world.ore.insert(
        here,
        crate::world::Ore {
            data: ore.data - 1,
            ..ore
        },
    );
    let o = &mut world.objects[i];
    o.cargo += 1;
    o.cargo_value += ore.bail_value(rules);
    o.harvest.stage_from = Some(frame);
    true
}

/// A refinery's queueing cell (`QueueingCell=` from its top-left), and the
/// pad beside it the harvester backs onto. `[observed]` (the pad is the
/// cell west of the queueing cell on the Soviet refinery)
fn dock_cells(world: &World, rules: &Rules, refinery: u32) -> Option<((i16, i16), (i16, i16))> {
    let b = world.objects.iter().find(|o| o.id == refinery)?;
    let t = &rules.technos[b.techno];
    let (w, h) = t.foundation;
    let x0 = (b.coord.x - w * LEPTONS_PER_CELL / 2).div_euclid(LEPTONS_PER_CELL);
    let y0 = (b.coord.y - h * LEPTONS_PER_CELL / 2).div_euclid(LEPTONS_PER_CELL);
    let (qx, qy) = t.queueing_cell.unwrap_or((w, 1));
    let queue = (i16::try_from(x0 + qx).ok()?, i16::try_from(y0 + qy).ok()?);
    Some((queue, (queue.0 - 1, queue.1)))
}

/// The refinery visit, a frame at a time. `[observed]`
fn visit(world: &mut World, rules: &Rules, frame: u32, i: usize) {
    let Some((refinery, step)) = world.objects[i].harvest.visit else {
        return;
    };
    let Some((queue, pad)) = dock_cells(world, rules, refinery) else {
        world.objects[i].harvest.visit = None;
        world.objects[i].harvest.state = State::Idle;
        return;
    };
    let t = &rules.technos[world.objects[i].techno];
    let owner = world.objects[i].owner;
    let o = &mut world.objects[i];
    let next = match step {
        Visit::Asked(at) if frame >= at => {
            o.harvest.dest = Some(queue);
            Visit::Approach
        }
        Visit::Approach if o.harvest.dest.is_none() => Visit::TurnRound,
        Visit::TurnRound => {
            // Face away from the pad, to back onto it.
            let want = step_facing(i32::from(queue.0 - pad.0), i32::from(queue.1 - pad.1));
            o.dir = u16::try_from(turn(i32::from(o.dir), want, t.rot * 256)).unwrap_or(0);
            if i32::from(o.dir) == want {
                let to = cell_centre(Coord {
                    x: i32::from(pad.0) * LEPTONS_PER_CELL,
                    y: i32::from(pad.1) * LEPTONS_PER_CELL,
                    z: o.coord.z,
                });
                o.motion.leg = Some((o.coord, to, 0));
                o.motion.carry = 0;
                Visit::Reverse
            } else {
                step
            }
        }
        Visit::Reverse => {
            if travel(o, DOCK_SPEED) {
                Visit::Unload(frame)
            } else {
                step
            }
        }
        Visit::Unload(since) if frame >= since + UNLOAD_FRAMES => {
            let value = o.cargo_value;
            o.cargo = 0;
            o.cargo_value = 0;
            if let Some(h) = world
                .houses
                .get_mut(usize::try_from(owner).unwrap_or(usize::MAX))
            {
                h.credits += value;
            }
            Visit::Paid(frame + LEAVE_FRAMES)
        }
        Visit::Paid(at) if frame >= at => {
            let h = &mut o.harvest;
            h.visit = None;
            h.dest = h.archive.take();
            h.state = State::Look;
            h.next_look = frame;
            o.motion.rolling = false;
            return;
        }
        other => other,
    };
    world.objects[i].harvest.visit = Some((refinery, next));
}

/// Drive toward `dest` along a planned route: turn on the spot to the
/// first step, then cell centre to cell centre, as hunters do.
fn drive(world: &mut World, rules: &Rules, i: usize) {
    let Some(goal) = world.objects[i].harvest.dest else {
        return;
    };
    let t = &rules.technos[world.objects[i].techno];
    let cells = &world.cells;
    let o = &mut world.objects[i];
    if o.harvest.planned != Some(goal) {
        o.harvest.planned = Some(goal);
        o.motion.route.clear();
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
            o.harvest.dest = None;
            o.harvest.planned = None;
            o.motion.rolling = false;
            return;
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
            let speed = throttle(o, t, distance(to_goal, o.coord));
            o.motion.rolling = true;
            if travel(o, speed) && to_cell(o.coord) == goal {
                o.harvest.dest = None;
                o.harvest.planned = None;
                o.motion.rolling = false;
                o.motion.route.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::{Object, Ore};
    use ra2_rules::{Ini, Rules};

    fn rules() -> Rules {
        Rules::from_ini(
            &Ini::parse(
                b"[VehicleTypes]\n1=HARV\n[BuildingTypes]\n1=NAREFN\n\
                  [HARV]\nStrength=1000\nSpeed=4\nROT=5\nHarvester=yes\nStorage=40\nDock=NAREFN\n\
                  [NAREFN]\nStrength=1000\n[Riparius]\nValue=25\n",
            ),
            &Ini::parse(b"[NAREFN]\nFoundation=4x3\nQueueingCell=4,1\n"),
        )
    }

    fn miner(world: &mut World, rules: &Rules, cell: (i16, i16)) -> usize {
        let p = ra2_rules::scenario::Placement {
            kind: Kind::Vehicle,
            type_id: "HARV".into(),
            owner: "Neutral".into(),
            cell,
            sub_cell: 0,
            health: 256,
            facing: 64,
            mission: "Harvest".into(),
        };
        world.place(rules, &p, 0);
        world.objects.len() - 1
    }

    fn field(world: &mut World, x: i16, y: i16, w: i16, h: i16) {
        for cy in y..y + h {
            for cx in x..x + w {
                world.ore.insert(
                    (cx, cy),
                    Ore {
                        overlay: 102,
                        data: 11,
                    },
                );
            }
        }
    }

    /// The cells the recorded War Miner chose, in order, as it worked
    /// through the 5×5 field from its north-west corner.
    #[test]
    fn the_search_picks_the_cells_the_original_picked() {
        let rules = rules();
        let mut world = World::default();
        world.add_house("Neutral", 0);
        field(&mut world, 52, 44, 5, 5);
        let i = miner(&mut world, &rules, (44, 49));
        assert_eq!(find_ore(&world, &rules, i, 48), Some((52, 44)));
        let mut at = (52, 44);
        for want in [
            (52, 45),
            (53, 44),
            (53, 45),
            (52, 46),
            (52, 47),
            (53, 46),
            (54, 45),
        ] {
            world.ore.remove(&at);
            let o: &mut Object = &mut world.objects[i];
            o.coord = cell_centre(Coord {
                x: i32::from(at.0) * LEPTONS_PER_CELL,
                y: i32::from(at.1) * LEPTONS_PER_CELL,
                z: 0,
            });
            assert_eq!(find_ore(&world, &rules, i, 6), Some(want), "from {at:?}");
            at = want;
        }
    }

    #[test]
    fn a_cell_gives_its_data_in_bails_19_frames_apart_then_goes() {
        let rules = rules();
        let mut world = World::default();
        world.add_house("Neutral", 0);
        field(&mut world, 10, 10, 1, 1);
        let i = miner(&mut world, &rules, (10, 10));
        let mut rng = Randomizer::new(1);
        let mut bails = Vec::new();
        for frame in 1..=300 {
            let before = world.objects[i].cargo;
            update(&mut world, &rules, &mut rng, frame);
            if world.objects[i].cargo > before {
                bails.push(frame);
            }
        }
        assert_eq!(bails.len(), 11);
        assert!(bails.windows(2).all(|w| w[1] - w[0] == 19), "{bails:?}");
        assert_eq!(bails[0], FIRST_LOOK + 19);
        assert!(world.ore.is_empty(), "the twelfth try removes the overlay");
        assert_eq!(world.objects[i].cargo_value, 11 * 25);
    }
}
