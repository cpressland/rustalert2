//! Orders a player gives a unit: move to a cell, attack, enter, capture.
//! See `docs/spec/locomotors.md` (moving) and `docs/spec/repair.md`
//! (entering a Service Depot).

use ra2_rules::Rules;

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::hunt::{cell_centre, distance, step_facing, throttle, to_cell, travel, turn};
use crate::pathfind;
use crate::rng::Randomizer;
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
    // Hunt takes an attack order's target from the order, as a new one,
    // and goes to it afresh.
    o.motion.target = None;
    o.motion.arrived = false;
    o.motion.goal = None;
}

/// One frame of every commanded unit.
pub fn update(world: &mut World, rules: &Rules, rng: &mut Randomizer, frame: u32) {
    let mut gone = Vec::new();
    for i in 0..world.objects.len() {
        let command = world.objects[i].command;
        match command {
            Some(Command::Move(goal)) => {
                if drive(world, rules, Some(&mut *rng), i, goal) {
                    let o = &mut world.objects[i];
                    o.command = None;
                    o.mission = Mission::Guard;
                }
            }
            Some(Command::Capture(id)) if capture(world, rules, frame, i, id) => {
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
    if depot.passengers > 0 {
        board(world, rules, i, b);
        return;
    }
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
    if drive(world, rules, None, i, pad) {
        world.objects[i].depot = Some(DepotVisit {
            next: frame + DEPOT_FIRST_STEP,
            steps: 0,
            whole_at: None,
        });
    }
}

/// Infantry boarding a transport (`Passengers=`): it walks to the
/// transport's cell and goes in on reaching it, if there's room.
/// `[observed]` (`allied-ifv-repair`: an engineer into an IFV)
fn board(world: &mut World, rules: &Rules, i: usize, b: usize) {
    let room = usize::try_from(rules.technos[world.objects[b].techno].passengers).unwrap_or(0);
    if world.objects[b].owner != world.objects[i].owner || world.objects[b].passengers.len() >= room
    {
        stand(world, i);
        return;
    }
    let goal = to_cell(world.objects[b].coord);
    if to_cell(world.objects[i].coord) == goal || drive(world, rules, None, i, goal) {
        let id = world.objects[i].id;
        let o = &mut world.objects[i];
        o.limbo = true;
        o.command = None;
        o.mission = Mission::Sleep;
        o.motion = crate::hunt::Motion::default();
        world.objects[b].passengers.push(id);
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

/// A spy goes into an enemy's building and is used up; what its house
/// gains depends on the building. A factory of infantry (a barracks): the
/// house's infantry are built veterans from then on. `[observed]`
/// (`allied-spy-barracks`) A factory of vehicles: its vehicles, the same
/// way. `[hypothesis]` A power plant: its owner makes no power for
/// `SpyPowerBlackout=` frames less one, from that frame. `[observed]`
/// (`allied-spy-power`: a Neutral plant's 200 gone from the frame the spy
/// went in, back 999 frames on) A refinery: see below. Other buildings
/// aren't modelled yet.
fn infiltrate(world: &mut World, rules: &Rules, frame: u32, owner: u32, b: usize) {
    let t = &rules.technos[world.objects[b].techno];
    if t.refinery {
        // A refinery: `SpyMoneyStealPercent=` of its owner's money,
        // truncated, goes to the spy's house. `[observed]`
        // (`allied-spy-refinery`, recorded only: 42574 from the AI, 21287
        // taken, a spy in a Conscript's look having walked through its base)
        let victim = usize::try_from(world.objects[b].owner).unwrap_or(usize::MAX);
        let to = usize::try_from(owner).unwrap_or(usize::MAX);
        let taken = world
            .houses
            .get(victim)
            .map_or(0, |h| rules.general.spy_money_steal.of(h.credits.max(0)));
        if let Some(h) = world.houses.get_mut(victim) {
            h.credits -= taken;
        }
        if let Some(h) = world.houses.get_mut(to) {
            h.credits += taken;
        }
        return;
    }
    if t.power > 0 {
        let victim = usize::try_from(world.objects[b].owner).unwrap_or(usize::MAX);
        let lasts = u32::try_from(rules.general.spy_power_blackout).unwrap_or(0);
        if let Some(h) = world.houses.get_mut(victim) {
            h.blackout = Some((frame, (frame + lasts).saturating_sub(1)));
        }
        crate::production::power(world, rules, frame);
        return;
    }
    let Some(kind) = t.factory else {
        return;
    };
    if !matches!(kind, ra2_rules::Kind::Infantry | ra2_rules::Kind::Vehicle) {
        return;
    }
    if let Some(h) = world
        .houses
        .get_mut(usize::try_from(owner).unwrap_or(usize::MAX))
        && !h.veteran_kinds.contains(&kind)
    {
        h.veteran_kinds.push(kind);
    }
}

/// A C4 carrier (`C4=yes`, Tanya) attacking a building: it walks to the
/// building's nearest cell as an engineer does, and on arriving plants
/// its C4; the building goes `production::c4_frames` later, leaving no
/// crew. It then stands. `[observed]` (`allied-tanya-c4`: Tanya stopped
/// in a power plant's corner cell; the plant was gone 26 frames on)
pub(crate) fn sapper(world: &mut World, rules: &Rules, frame: u32, i: usize, id: u32) {
    let Some(b) = world.objects.iter().position(|o| o.id == id && !o.limbo) else {
        stand(world, i);
        return;
    };
    let Some(goal) = nearest_cell(world, rules, i, b) else {
        return;
    };
    if drive(world, rules, None, i, goal) {
        let delay = u32::try_from(ra2_rules::production::c4_frames(rules)).unwrap_or(0);
        if world.objects[b].c4_at.is_none() {
            world.objects[b].c4_at = Some(frame + delay);
        }
        stand(world, i);
    }
}

/// The cell of building `b`'s foundation nearest object `i`'s cell.
fn nearest_cell(world: &World, rules: &Rules, i: usize, b: usize) -> Option<(i16, i16)> {
    let (x0, y0) = crate::production::top_left(rules, &world.objects[b]);
    let (w, h) = rules.technos[world.objects[b].techno].foundation;
    let here = to_cell(world.objects[i].coord);
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
    best.map(|(c, _)| c)
}

/// An engineer on its way to a building: it walks to the building's
/// nearest cell and goes in when it reaches one. Its own side's building
/// is repaired to full strength, at no cost, and the engineer is used up.
/// `[observed]` (`yuri-engineer-repair`: a Bio Reactor at 500 of 700, whole
/// on the frame the engineer reached its top-left cell) An enemy's building
/// is captured. True once it's gone in.
fn capture(world: &mut World, rules: &Rules, frame: u32, i: usize, id: u32) -> bool {
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
        let owner = world.objects[i].owner;
        let room =
            usize::try_from(rules.technos[world.objects[b].techno].max_occupants).unwrap_or(0);
        if rules.technos[world.objects[i].techno].infiltrate && world.objects[b].owner != owner {
            infiltrate(world, rules, frame, owner, b);
            return true;
        }
        if rules.technos[world.objects[i].techno].occupier && room > 0 {
            // A garrison: the infantryman goes inside, and the building is
            // his house's while it's held. `[observed]` (`allied-garrison`:
            // a GI into a Neutral civilian house)
            if world.objects[b].passengers.len() < room {
                let id = world.objects[i].id;
                world.objects[b].owner = owner;
                world.objects[b].passengers.push(id);
                let o = &mut world.objects[i];
                o.limbo = true;
                o.command = None;
                o.mission = Mission::Sleep;
                o.motion = crate::hunt::Motion::default();
            } else {
                stand(world, i);
            }
            return false;
        }
        if world.objects[b].owner == owner {
            let strength = rules.technos[world.objects[b].techno].strength;
            world.objects[b].health = strength;
        } else {
            // An enemy's building becomes the engineer's house's, as it
            // stands, and both houses' power is counted again at once.
            // `[observed]` (`allied-engineer-capture`: a Neutral power
            // plant, 750 of 750; the player's output 200 to 400 on the same
            // frame)
            world.objects[b].owner = owner;
            world.objects[b].repairing = false;
            crate::production::power(world, rules, frame);
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
    drive(world, rules, None, i, goal);
    false
}

/// Drive toward `goal` along a planned route: turn on the spot to the
/// first step, then cell centre to cell centre, as harvesters do. True
/// once there.
pub(crate) fn drive(
    world: &mut World,
    rules: &Rules,
    rng: Option<&mut Randomizer>,
    i: usize,
    goal: (i16, i16),
) -> bool {
    // A teleporter warps there on the frame after the order, onto a
    // sub-cell spot, and waits before it can act. `[observed]`
    // (`allied-chrono-legion`)
    if rules.technos[world.objects[i].techno].teleporter {
        let o = &mut world.objects[i];
        if o.motion.planned != Some(goal) {
            o.motion.planned = Some(goal);
            return false;
        }
        let to = Coord {
            x: i32::from(goal.0) * LEPTONS_PER_CELL + 192,
            y: i32::from(goal.1) * LEPTONS_PER_CELL + 64,
            z: o.coord.z,
        };
        o.coord = to;
        o.motion.planned = None;
        return true;
    }
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
        let into = to_cell(next);
        if let Some(rng) = rng
            && world.crates.contains_key(&into)
            && crate::crates::enter(world, rules, rng, i, into)
        {
            let o = &mut world.objects[i];
            o.motion.leg = None;
            o.motion.route.clear();
            o.motion.planned = None;
            o.motion.rolling = false;
            return true;
        }
    }
    let t = &rules.technos[world.objects[i].techno];
    let o = &mut world.objects[i];
    if let Some((from, to, _)) = o.motion.leg {
        let want = step_facing(to.x - from.x, to.y - from.y);
        let facing = i32::from(o.dir);
        if o.kind == ra2_rules::Kind::Infantry {
            // Infantry turn at once and walk. `[observed]` (as hunting)
            o.dir = u16::try_from(want).unwrap_or(0);
            let speed = crate::hunt::object_speed(rules, t, o) * crate::hunt::WALK_SPEED;
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

#[cfg(test)]
mod tests {
    use super::*;
    use ra2_rules::Ini;

    /// The original's numbers: the AI had 42574 when a spy in a
    /// Conscript's look got into its refinery; 21287 went to the spy's
    /// house (8200 before). `[observed]`
    #[test]
    fn a_spy_in_a_refinery_takes_half_its_owners_money() {
        let rules = Rules::from_ini(
            &Ini::parse(
                b"[BuildingTypes]\n1=NAREFN\n[NAREFN]\nRefinery=yes\nStrength=900\n\
                  [InfantryTypes]\n1=SPY\n[SPY]\nInfiltrate=yes\nStrength=100\n",
            ),
            &Ini::parse(b""),
        );
        let mut world = World::default();
        world.add_house("British", 8200);
        world.add_house("Russians", 42574);
        let refinery = world.spawn(&rules, 0, 1, Coord::default());
        let b = world
            .objects
            .iter()
            .position(|o| o.id == refinery)
            .expect("placed");
        infiltrate(&mut world, &rules, 100, 0, b);
        assert_eq!(world.houses[0].credits, 29_487);
        assert_eq!(world.houses[1].credits, 21_287);
    }
}
