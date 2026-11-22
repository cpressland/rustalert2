//! Building things: deploying an MCV, factories counting through their
//! steps and taking the cost a step at a time, queues, cancelling, placing
//! and selling buildings, units leaving their factory, and each house's
//! power. See `docs/spec/production.md`.

use ra2_rules::production::{STEPS, build_time, power_output, refund};
use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::hunt::{to_cell, travel};
use crate::world::{Mission, World};

/// Frames from the deploy taking effect to the building. `[observed]` (the
/// AMCV, in `allied-base`)
const DEPLOY_FRAMES: u32 = 26;
/// Where infantry come out of a barracks, from its top-left corner, in
/// leptons, and the spot they walk to in each cell of the row below it.
/// `[observed]` (`GAPILE`; the game's choice of cell looks random, see
/// production.md)
const INFANTRY_EXIT: (i32, i32) = (320, 448);
const INFANTRY_SPOT: (i32, i32) = (64, 192);
/// Infantry speed, in 1/256 leptons a frame per point of stored speed (as
/// hunt.rs).
const WALK_SPEED: i32 = 277;
/// Selling: frames until the building goes and the refund is paid, and
/// until its crew comes out, from its top-left corner in leptons, to walk
/// to sub-spot (192, 64) of the cell right of its top row. `[observed]`
/// (`GAPOWR`)
const SELL_FRAMES: u32 = 50;
const CREW_FRAMES: u32 = 2;
const CREW_EXIT: (i32, i32) = (455, 313);
const CREW_SPOT: (i32, i32) = (192, 64);
/// A building's `FreeUnit=` appears this many frames after it's placed, in
/// the cell up and left of its top-left. `[observed]` (a refinery's Chrono
/// Miner; likely the end of the build-up)
const FREE_UNIT_FRAMES: u32 = 50;
/// A vehicle appears at its war factory's centre, waits this many frames
/// (the door), then drives out to the cell beside the factory's middle
/// row. `[observed]` (a Grizzly from `GAWEAP`; it drives a little slower
/// than on open ground, so it arrives a few frames late in ours)
const VEHICLE_DOOR_FRAMES: u32 = 48;

/// One house's production of one thing, or a thing queued behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Factory {
    pub house: u32,
    pub kind: Kind,
    pub techno: usize,
    /// The object being built, in limbo; `None` while queued.
    pub object: Option<u32>,
    /// Frames per step, and the frame of the next step.
    pub rate: i32,
    pub next: u32,
    /// Steps done, of [`STEPS`].
    pub stage: i32,
    /// Credits still to pay.
    pub owed: i32,
    /// A finished unit leaves on this frame.
    pub exit_at: Option<u32>,
    /// Held since this frame.
    pub held: Option<u32>,
}

/// An order to build something, as a player gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Build {
    Deploy {
        house: u32,
        techno: usize,
    },
    Produce {
        house: u32,
        techno: usize,
    },
    Place {
        house: u32,
        techno: usize,
        cell: (i16, i16),
    },
    Sell {
        house: u32,
        techno: usize,
    },
    Abandon {
        house: u32,
        techno: usize,
    },
    Hold {
        house: u32,
        techno: usize,
    },
}

/// A building's top-left cell.
pub fn top_left(rules: &Rules, o: &crate::world::Object) -> (i32, i32) {
    let (w, h) = rules.technos[o.techno].foundation;
    (
        (o.coord.x - w * LEPTONS_PER_CELL / 2).div_euclid(LEPTONS_PER_CELL),
        (o.coord.y - h * LEPTONS_PER_CELL / 2).div_euclid(LEPTONS_PER_CELL),
    )
}

/// A building's centre with its top-left at `cell`.
fn centre(rules: &Rules, techno: usize, cell: (i32, i32), z: i32) -> Coord {
    let (w, h) = rules.technos[techno].foundation;
    Coord {
        x: cell.0 * LEPTONS_PER_CELL + w * LEPTONS_PER_CELL / 2,
        y: cell.1 * LEPTONS_PER_CELL + h * LEPTONS_PER_CELL / 2,
        z,
    }
}

/// A house's standing buildings that make things of `kind`, oldest first.
fn factories_of(world: &World, rules: &Rules, house: u32, kind: Kind) -> Vec<usize> {
    (0..world.objects.len())
        .filter(|&i| {
            let o = &world.objects[i];
            o.owner == house
                && !o.limbo
                && o.kind == Kind::Building
                && rules.technos[o.techno].factory == Some(kind)
        })
        .collect()
}

/// Whether a house has what `techno` needs: for each `Prerequisite=`, a
/// standing building of that type, or of the group (`POWER`, `FACTORY`…)
/// `[General]` lists for it. `[community]` (the groups are the rules'
/// `Prerequisite<Name>=` keys)
fn has_prerequisites(world: &World, rules: &Rules, house: u32, techno: usize) -> bool {
    let owned = |id: &str| {
        world.objects.iter().any(|o| {
            o.owner == house
                && !o.limbo
                && o.kind == Kind::Building
                && rules.technos[o.techno].id.eq_ignore_ascii_case(id)
        })
    };
    rules.technos[techno].prerequisite.iter().all(|p| {
        match rules
            .general
            .prerequisite_groups
            .iter()
            .find(|(g, _)| g.eq_ignore_ascii_case(p))
        {
            Some((_, members)) => members.iter().any(|m| owned(m)),
            None => owned(p),
        }
    })
}

/// The time a house takes to build `techno` now, and its step length.
fn rate_for(world: &World, rules: &Rules, house: u32, techno: usize) -> i32 {
    let t = &rules.technos[techno];
    let count = factories_of(world, rules, house, t.kind).len().max(1);
    let h = &world.houses[usize::try_from(house).unwrap_or(0)];
    let country = rules.countries.iter().find(|c| c.id == h.country);
    let time = build_time(
        rules,
        t,
        country,
        h.power_output,
        h.power_drain,
        i32::try_from(count).unwrap_or(1),
    );
    // At least 1, at most 255 frames a step. `[recomp 0x004C9F07]`
    (time / STEPS).clamp(1, 255)
}

/// Start the house's next queued item of a kind, if nothing of the kind is
/// building.
fn start_next(world: &mut World, rules: &Rules, frame: u32, house: u32, kind: Kind) {
    if world
        .factories
        .iter()
        .any(|f| f.house == house && f.kind == kind && f.object.is_some())
    {
        return;
    }
    let Some(k) = world
        .factories
        .iter()
        .position(|f| f.house == house && f.kind == kind)
    else {
        return;
    };
    let techno = world.factories[k].techno;
    let rate = rate_for(world, rules, house, techno);
    let object = world.spawn(rules, techno, house, Coord::default());
    if let Some(o) = world.objects.iter_mut().find(|o| o.id == object) {
        o.limbo = true;
    }
    let f = &mut world.factories[k];
    f.object = Some(object);
    f.rate = rate;
    f.next = frame + u32::try_from(rate).unwrap_or(1);
}

/// Stop building `factories[k]`: what's been paid comes back, and the
/// object goes.
fn abandon(world: &mut World, rules: &Rules, frame: u32, k: usize) {
    let f = world.factories.remove(k);
    if let Some(id) = f.object {
        world.objects.retain(|o| o.id != id);
        let paid = rules.technos[f.techno].cost - f.owed;
        if let Some(h) = world
            .houses
            .get_mut(usize::try_from(f.house).unwrap_or(usize::MAX))
        {
            h.credits += paid;
        }
        start_next(world, rules, frame, f.house, f.kind);
    }
}

/// Whether the house may put `techno` with its top-left at `cell`: some
/// cell within `Adjacent=` + 1 cells round its foundation holds one of the
/// house's `BaseNormal=` buildings (`[recomp 0x004A8EB0]`), and no
/// building or vehicle stands on the foundation (`[hypothesis]`; the
/// game's terrain checks aren't traced).
fn can_place(world: &World, rules: &Rules, house: u32, techno: usize, cell: (i32, i32)) -> bool {
    let t = &rules.technos[techno];
    let (w, h) = t.foundation;
    let reach = t.adjacent + 1;
    let covers = |o: &crate::world::Object, x: i32, y: i32| {
        let (bx, by) = top_left(rules, o);
        let (bw, bh) = rules.technos[o.techno].foundation;
        (bx..bx + bw).contains(&x) && (by..by + bh).contains(&y)
    };
    let mut near = false;
    for o in &world.objects {
        if o.limbo {
            continue;
        }
        if o.kind == Kind::Building {
            for y in cell.1..cell.1 + h {
                for x in cell.0..cell.0 + w {
                    if covers(o, x, y) {
                        return false;
                    }
                }
            }
            if o.owner == house && rules.technos[o.techno].base_normal {
                let (bx, by) = top_left(rules, o);
                let (bw, bh) = rules.technos[o.techno].foundation;
                near |= bx < cell.0 + w + reach
                    && cell.0 - reach < bx + bw
                    && by < cell.1 + h + reach
                    && cell.1 - reach < by + bh;
            }
        } else if o.kind == Kind::Vehicle {
            let (x, y) = to_cell(o.coord);
            if (cell.0..cell.0 + w).contains(&i32::from(x))
                && (cell.1..cell.1 + h).contains(&i32::from(y))
            {
                return false;
            }
        }
    }
    near
}

/// Apply an order, on the frame it takes effect.
#[allow(clippy::too_many_lines)] // one arm per order, as the game's event switch
pub fn order(world: &mut World, rules: &Rules, frame: u32, build: Build) {
    match build {
        Build::Deploy { house, techno } => {
            let unit = world.objects.iter().find(|o| {
                o.owner == house
                    && o.techno == techno
                    && !o.limbo
                    && rules.technos[o.techno].deploys_into.is_some()
            });
            if let Some(u) = unit
                && !world.deploying.iter().any(|&(id, _)| id == u.id)
            {
                let id = u.id;
                world.deploying.push((id, frame + DEPLOY_FRAMES));
            }
        }
        Build::Produce { house, techno } => {
            // Producing something held resumes it: its next step is a rate
            // from now. `[observed]`
            if let Some(f) = world
                .factories
                .iter_mut()
                .find(|f| f.house == house && f.techno == techno && f.held.is_some())
            {
                f.held = None;
                f.next = frame + u32::try_from(f.rate).unwrap_or(1);
                return;
            }
            let t = &rules.technos[techno];
            if factories_of(world, rules, house, t.kind).is_empty()
                || !has_prerequisites(world, rules, house, techno)
            {
                return;
            }
            let busy = world
                .factories
                .iter()
                .any(|f| f.house == house && f.kind == t.kind);
            // Buildings build one at a time; others queue. `[observed]`
            if busy && t.kind == Kind::Building {
                return;
            }
            world.factories.push(Factory {
                house,
                kind: t.kind,
                techno,
                object: None,
                rate: 1,
                next: 0,
                stage: 0,
                owed: t.cost,
                exit_at: None,
                held: None,
            });
            start_next(world, rules, frame, house, t.kind);
        }
        Build::Abandon { house, techno } => {
            // The last queued one first, else the one building.
            let queued = world
                .factories
                .iter()
                .rposition(|f| f.house == house && f.techno == techno && f.object.is_none());
            if let Some(k) = queued {
                world.factories.remove(k);
            } else if let Some(k) = world
                .factories
                .iter()
                .position(|f| f.house == house && f.techno == techno)
            {
                abandon(world, rules, frame, k);
            }
        }
        Build::Hold { house, techno } => {
            // It stops after this frame's step. `[observed]`
            if let Some(f) = world
                .factories
                .iter_mut()
                .find(|f| f.house == house && f.techno == techno && f.object.is_some())
            {
                f.held = Some(frame);
            }
        }
        Build::Sell { house, techno } => {
            if let Some(b) = world.objects.iter_mut().find(|o| {
                o.owner == house
                    && o.techno == techno
                    && o.kind == Kind::Building
                    && !o.limbo
                    && o.sold_at.is_none()
            }) {
                b.sold_at = Some(frame);
            }
        }
        Build::Place {
            house,
            techno,
            cell,
        } => {
            let Some(k) = world.factories.iter().position(|f| {
                f.house == house
                    && f.techno == techno
                    && f.kind == Kind::Building
                    && f.stage >= STEPS
            }) else {
                return;
            };
            let cell = (i32::from(cell.0), i32::from(cell.1));
            if !can_place(world, rules, house, techno, cell) {
                return;
            }
            let f = world.factories.remove(k);
            let c = centre(rules, techno, cell, 0);
            if let Some(o) = world.objects.iter_mut().find(|o| Some(o.id) == f.object) {
                o.limbo = false;
                o.coord = c;
                o.placed = frame;
            }
            start_next(world, rules, frame, house, Kind::Building);
        }
    }
}

/// One frame of production, deploying, selling, units leaving factories,
/// and power.
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    deploy(world, rules, frame);
    for k in 0..world.factories.len() {
        let f = &mut world.factories[k];
        if f.object.is_none()
            || f.stage >= STEPS
            || frame != f.next
            || f.held.is_some_and(|h| h < frame)
        {
            continue;
        }
        // The next step is timed at the rate as it stands now; a change of
        // rate waits for it. `[recomp 0x004CA6E0]`
        f.next = frame + u32::try_from(f.rate).unwrap_or(1);
        // Each step pays what's still owed over the payments left (the
        // last, all that's left). A house that can't afford it doesn't take
        // the step, and tries again a rate later. `[recomp 0x004C9B20]`
        let stage = f.stage + 1;
        let pay = if stage < STEPS {
            f.owed / (STEPS - stage)
        } else {
            f.owed
        }
        .min(f.owed);
        let Some(h) = world
            .houses
            .get_mut(usize::try_from(f.house).unwrap_or(usize::MAX))
        else {
            continue;
        };
        if pay > h.credits {
            continue;
        }
        h.credits -= pay;
        f.owed -= pay;
        f.stage = stage;
        if f.stage >= STEPS && f.kind != Kind::Building {
            f.exit_at = Some(frame + 1);
        }
    }
    let leaving: Vec<usize> = (0..world.factories.len())
        .filter(|&k| world.factories[k].exit_at == Some(frame))
        .collect();
    for &k in leaving.iter().rev() {
        let f = world.factories.remove(k);
        leave(world, rules, frame, &f);
        start_next(world, rules, frame, f.house, f.kind);
    }
    sell(world, rules, frame);
    free_units(world, rules, frame);
    // Production whose prerequisites have gone is cancelled. `[observed]`
    // (a refinery, when its power plant was sold)
    while let Some(k) = (0..world.factories.len()).find(|&k| {
        let f = &world.factories[k];
        f.object.is_some() && !has_prerequisites(world, rules, f.house, f.techno)
    }) {
        abandon(world, rules, frame, k);
    }
    walk_out(world, rules, frame);
    power(world, rules, frame);
}

/// Deploys that are due: the unit goes and its building stands centred on
/// its cell (top-left half the foundation up and left). `[observed]`
fn deploy(world: &mut World, rules: &Rules, frame: u32) {
    let due: Vec<u32> = world
        .deploying
        .iter()
        .filter(|&&(_, at)| at == frame)
        .map(|&(id, _)| id)
        .collect();
    world.deploying.retain(|&(_, at)| at != frame);
    for id in due {
        let Some(i) = world.objects.iter().position(|o| o.id == id) else {
            continue;
        };
        let unit = world.objects.remove(i);
        let Some(into) = rules.technos[unit.techno]
            .deploys_into
            .as_ref()
            .and_then(|d| {
                rules
                    .technos
                    .iter()
                    .position(|t| t.id.eq_ignore_ascii_case(d))
            })
        else {
            continue;
        };
        let (w, h) = rules.technos[into].foundation;
        let (cx, cy) = to_cell(unit.coord);
        let cell = (i32::from(cx) - (w - 1) / 2, i32::from(cy) - (h - 1) / 2);
        let c = centre(rules, into, cell, unit.coord.z);
        let b = world.spawn(rules, into, unit.owner, c);
        if let Some(o) = world.objects.iter_mut().find(|o| o.id == b) {
            o.placed = frame;
        }
    }
}

/// Sales in progress: the crew comes out, then the building goes and the
/// refund is paid. `[observed]` (one crew soldier from a power plant;
/// how many come out isn't traced)
fn sell(world: &mut World, rules: &Rules, frame: u32) {
    let selling: Vec<(u32, u32)> = world
        .objects
        .iter()
        .filter_map(|o| o.sold_at.map(|s| (o.id, s)))
        .collect();
    for (id, sold) in selling {
        let Some(i) = world.objects.iter().position(|o| o.id == id) else {
            continue;
        };
        if frame == sold + CREW_FRAMES {
            let b = &world.objects[i];
            let (x0, y0) = top_left(rules, b);
            let (w, _) = rules.technos[b.techno].foundation;
            let owner = b.owner;
            let soviet = world
                .houses
                .get(usize::try_from(owner).unwrap_or(usize::MAX))
                .and_then(|h| rules.countries.iter().find(|c| c.id == h.country))
                .is_some_and(|c| c.side.eq_ignore_ascii_case("Nod"));
            let crew = if soviet {
                &rules.general.soviet_crew
            } else {
                &rules.general.allied_crew
            };
            if let Some(t) = crew.as_ref().and_then(|c| {
                rules
                    .technos
                    .iter()
                    .position(|t| t.id.eq_ignore_ascii_case(c))
            }) {
                let at = Coord {
                    x: x0 * LEPTONS_PER_CELL + CREW_EXIT.0,
                    y: y0 * LEPTONS_PER_CELL + CREW_EXIT.1,
                    z: 0,
                };
                let id = world.spawn(rules, t, owner, at);
                if let Some(o) = world.objects.iter_mut().find(|o| o.id == id) {
                    // Walks from the next frame.
                    o.placed = frame - 1;
                    o.motion.goal = Some(Coord {
                        x: (x0 + w) * LEPTONS_PER_CELL + CREW_SPOT.0,
                        y: y0 * LEPTONS_PER_CELL + CREW_SPOT.1,
                        z: 0,
                    });
                }
            }
        }
        if frame == sold + SELL_FRAMES {
            let b = world.objects.remove(i);
            let paid = refund(rules, &rules.technos[b.techno]);
            if let Some(h) = world
                .houses
                .get_mut(usize::try_from(b.owner).unwrap_or(usize::MAX))
            {
                h.credits += paid;
            }
        }
    }
}

/// Units that come free with buildings placed `FREE_UNIT_FRAMES` ago.
fn free_units(world: &mut World, rules: &Rules, frame: u32) {
    let due: Vec<(usize, u32, (i32, i32))> = world
        .objects
        .iter()
        .filter(|o| {
            o.kind == Kind::Building
                && !o.limbo
                && o.placed > 0
                && o.placed + FREE_UNIT_FRAMES == frame
        })
        .filter_map(|o| {
            let free = rules.technos[o.techno].free_unit.as_ref()?;
            let t = rules
                .technos
                .iter()
                .position(|t| t.id.eq_ignore_ascii_case(free))?;
            Some((t, o.owner, top_left(rules, o)))
        })
        .collect();
    for (t, owner, (x0, y0)) in due {
        let at = Coord {
            x: (x0 - 1) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            y: (y0 - 1) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            z: 0,
        };
        let id = world.spawn(rules, t, owner, at);
        if rules.technos[t].harvester
            && let Some(o) = world.objects.iter_mut().find(|o| o.id == id)
        {
            o.mission = Mission::Harvest;
        }
    }
}

/// A finished unit comes out of its house's oldest factory of its kind.
fn leave(world: &mut World, rules: &Rules, frame: u32, f: &Factory) {
    let Some(&fi) = factories_of(world, rules, f.house, f.kind).first() else {
        return;
    };
    let (x0, y0) = top_left(rules, &world.objects[fi]);
    let (w, h) = rules.technos[world.objects[fi].techno].foundation;
    if f.kind != Kind::Infantry {
        let at = world.objects[fi].coord;
        let goal = Coord {
            x: (x0 + w) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            y: (y0 + h / 2) * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            z: 0,
        };
        if let Some(o) = world.objects.iter_mut().find(|o| Some(o.id) == f.object) {
            o.limbo = false;
            o.coord = Coord { z: 0, ..at };
            o.placed = frame;
            o.motion.goal = Some(goal);
            // Facing east, out of the door.
            o.dir = 16_384;
        }
        return;
    }
    let at = Coord {
        x: x0 * LEPTONS_PER_CELL + INFANTRY_EXIT.0,
        y: y0 * LEPTONS_PER_CELL + INFANTRY_EXIT.1,
        z: 0,
    };
    // The first cell of the row below the factory with nobody standing or
    // heading there. `[observed]` (the game's pick varies; see the spec)
    let row = y0 + h;
    let taken = |x: i32| {
        world.objects.iter().any(|o| {
            o.kind == Kind::Infantry
                && !o.limbo
                && (to_cell(o.coord) == (cell_i16(x), cell_i16(row))
                    || o.motion
                        .goal
                        .is_some_and(|g| to_cell(g) == (cell_i16(x), cell_i16(row))))
        })
    };
    let x = (x0..x0 + w.max(1) + 8).find(|&x| !taken(x)).unwrap_or(x0);
    let goal = Coord {
        x: x * LEPTONS_PER_CELL + INFANTRY_SPOT.0,
        y: row * LEPTONS_PER_CELL + INFANTRY_SPOT.1,
        z: 0,
    };
    if let Some(o) = world.objects.iter_mut().find(|o| Some(o.id) == f.object) {
        o.limbo = false;
        o.coord = at;
        o.placed = frame;
        o.motion.goal = Some(goal);
    }
}

fn cell_i16(v: i32) -> i16 {
    i16::try_from(v).unwrap_or(0)
}

/// Units that just came out go to their spot: infantry walk, setting off
/// the frame after next; vehicles drive once the door is open.
/// `[observed]`
fn walk_out(world: &mut World, rules: &Rules, frame: u32) {
    for o in &mut world.objects {
        let wait = match o.kind {
            Kind::Infantry => 2,
            Kind::Vehicle => VEHICLE_DOOR_FRAMES,
            _ => continue,
        };
        if o.limbo || o.mission != Mission::Guard {
            continue;
        }
        let Some(goal) = o.motion.goal else {
            continue;
        };
        if frame < o.placed + wait {
            continue;
        }
        if o.motion.leg.is_none() {
            if o.coord == goal {
                o.motion.goal = None;
                continue;
            }
            o.motion.leg = Some((o.coord, goal, 0));
        }
        let t = &rules.technos[o.techno];
        let speed = if o.kind == Kind::Infantry {
            t.speed_fraction * WALK_SPEED
        } else {
            let left = crate::hunt::distance(goal, o.coord);
            crate::hunt::throttle(o, t, left)
        };
        if travel(o, speed) {
            o.motion.goal = None;
        }
    }
}

/// Each house's power: what its buildings make, × their health, from the
/// frame after they're placed, and what they draw, from the frame they're
/// placed. A change re-times the house's factories. `[recomp 0x00508C30]`
/// (timing `[observed]`)
fn power(world: &mut World, rules: &Rules, frame: u32) {
    let before: Vec<(i32, i32)> = world
        .houses
        .iter()
        .map(|h| (h.power_output, h.power_drain))
        .collect();
    for h in &mut world.houses {
        h.power_output = 0;
        h.power_drain = 0;
    }
    for o in &world.objects {
        if o.limbo || o.kind != Kind::Building {
            continue;
        }
        let t = &rules.technos[o.techno];
        let Some(h) = world
            .houses
            .get_mut(usize::try_from(o.owner).unwrap_or(usize::MAX))
        else {
            continue;
        };
        if t.power > 0 && frame > o.placed {
            h.power_output += power_output(t.power, o.health, t.strength);
        } else if t.power < 0 {
            h.power_drain -= t.power;
        }
    }
    for (index, old) in before.into_iter().enumerate() {
        let h = &world.houses[index];
        if (h.power_output, h.power_drain) == old {
            continue;
        }
        let house = h.index;
        for k in 0..world.factories.len() {
            if world.factories[k].house == house && world.factories[k].object.is_some() {
                let techno = world.factories[k].techno;
                world.factories[k].rate = rate_for(world, rules, house, techno);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick::{Order, Sim};
    use ra2_rules::Ini;

    /// The opening of `allied-base` from hand-written rules: deploy, a power
    /// plant paid a step at a time and placed, then a barracks.
    #[test]
    fn an_mcv_deploys_and_a_power_plant_is_paid_for_and_placed() {
        let rules = Rules::from_ini(
            &Ini::parse(
                b"[General]\nBuildSpeed=.7\nMultipleFactory=0.8\n\
                  [VehicleTypes]\n1=AMCV\n[BuildingTypes]\n1=GACNST\n2=GAPOWR\n3=GAPILE\n\
                  [AMCV]\nStrength=1000\nDeploysInto=GACNST\n\
                  [GACNST]\nStrength=1000\nFactory=BuildingType\nCost=3000\n\
                  [GAPOWR]\nStrength=750\nCost=800\nPower=200\n\
                  [GAPILE]\nStrength=500\nCost=500\nPower=-10\nFactory=InfantryType\n",
            ),
            &Ini::parse(
                b"[GACNST]\nFoundation=4x4\n[GAPOWR]\nFoundation=2x2\n[GAPILE]\nFoundation=3x2\n",
            ),
        );
        let id = |name: &str| {
            rules
                .technos
                .iter()
                .position(|t| t.id == name)
                .expect("type")
        };
        let (amcv, yard_t, plant_t) = (id("AMCV"), id("GACNST"), id("GAPOWR"));
        let mut world = World::default();
        world.add_house("British", 10_000);
        let mcv = world.spawn(
            &rules,
            amcv,
            0,
            Coord {
                x: 35 * 256 + 128,
                y: 35 * 256 + 128,
                z: 0,
            },
        );
        assert_eq!(mcv, 0);
        let mut sim = Sim::start(1, rules.clone(), world);
        let orders = [
            (
                30,
                Build::Deploy {
                    house: 0,
                    techno: amcv,
                },
            ),
            (
                100,
                Build::Produce {
                    house: 0,
                    techno: plant_t,
                },
            ),
            (
                600,
                Build::Place {
                    house: 0,
                    techno: plant_t,
                    cell: (38, 34),
                },
            ),
        ];
        let mut credits = Vec::new();
        while sim.frame() < 700 {
            let now: Vec<Order> = orders
                .iter()
                .filter(|(f, _)| *f == sim.frame())
                .map(|(_, b)| Order::Build(*b))
                .collect();
            sim.step(&now);
            credits.push(sim.world().houses[0].credits);
        }
        let w = sim.world();
        let yard = w
            .objects
            .iter()
            .find(|o| o.techno == yard_t)
            .expect("deployed");
        assert_eq!(top_left(&rules, yard), (34, 34));
        // Nine-frame steps from frame 101: 53 payments, the first on 110.
        assert_eq!(credits[108], 10_000);
        assert_eq!(credits[109], 10_000 - 15);
        assert_eq!(credits[589], 9_200);
        let plant = w
            .objects
            .iter()
            .find(|o| o.techno == plant_t)
            .expect("built");
        assert!(!plant.limbo);
        assert_eq!(top_left(&rules, plant), (38, 34));
        assert_eq!(
            (w.houses[0].power_output, w.houses[0].power_drain),
            (200, 0)
        );
    }

    #[test]
    fn a_house_that_cannot_pay_waits_a_step_and_tries_again() {
        let rules = Rules::from_ini(
            &Ini::parse(
                b"[General]\nBuildSpeed=.7\n[BuildingTypes]\n1=GACNST\n2=GAPOWR\n\
                  [GACNST]\nStrength=1000\nFactory=BuildingType\n[GAPOWR]\nStrength=750\nCost=800\n",
            ),
            &Ini::parse(b"[GACNST]\nFoundation=4x4\n[GAPOWR]\nFoundation=2x2\n"),
        );
        let mut world = World::default();
        world.add_house("British", 30);
        let yard = rules
            .technos
            .iter()
            .position(|t| t.id == "GACNST")
            .expect("yard");
        let plant = rules
            .technos
            .iter()
            .position(|t| t.id == "GAPOWR")
            .expect("plant");
        world.spawn(
            &rules,
            yard,
            0,
            Coord {
                x: 9216,
                y: 9216,
                z: 0,
            },
        );
        order(
            &mut world,
            &rules,
            1,
            Build::Produce {
                house: 0,
                techno: plant,
            },
        );
        for frame in 2..=40 {
            update(&mut world, &rules, frame);
        }
        // Two payments of 15 (frames 10 and 19), then nothing left.
        assert_eq!(world.houses[0].credits, 0);
        assert_eq!(world.factories[0].stage, 2);
        world.houses[0].credits = 100;
        for frame in 41..=46 {
            update(&mut world, &rules, frame);
        }
        // Retried every 9 frames (28, 37, 46): the third step on 46.
        assert_eq!(world.factories[0].stage, 3);
        assert_eq!(world.houses[0].credits, 85);
    }
}
