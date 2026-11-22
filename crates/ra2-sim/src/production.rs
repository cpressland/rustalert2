//! Building things: deploying an MCV, factories counting through their
//! steps and taking the cost a step at a time, placing buildings, units
//! leaving their factory, and each house's power. See
//! `docs/spec/production.md`.

use ra2_rules::production::{STEPS, build_time};
use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::hunt::{to_cell, travel};
use crate::world::World;

/// Frames from the deploy taking effect to the building. `[observed]` (the
/// AMCV, in `allied-base`)
const DEPLOY_FRAMES: u32 = 26;
/// Where infantry come out of a barracks, from its top-left corner, in
/// leptons, and the spot they walk to in each cell of the row below it.
/// `[observed]` (`GAPILE`)
const INFANTRY_EXIT: (i32, i32) = (320, 448);
const INFANTRY_SPOT: (i32, i32) = (64, 192);
/// Infantry speed, in 1/256 leptons a frame per point of stored speed (as
/// hunt.rs).
const WALK_SPEED: i32 = 277;

/// One house's production of one kind of thing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Factory {
    pub house: u32,
    pub kind: Kind,
    pub techno: usize,
    /// The object being built, in limbo.
    pub object: u32,
    /// The frame it started, and frames per step.
    pub start: u32,
    pub rate: i32,
    /// Steps done, of [`STEPS`].
    pub stage: i32,
    /// Credits still to pay.
    pub owed: i32,
    /// A finished unit leaves on this frame.
    pub exit_at: Option<u32>,
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

/// A house's buildings that make things of `kind`, oldest first.
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

/// Apply an order, on the frame it takes effect.
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
            let t = &rules.technos[techno];
            if world
                .factories
                .iter()
                .any(|f| f.house == house && f.kind == t.kind)
            {
                return; // one at a time per kind; queues aren't modelled yet
            }
            let count = factories_of(world, rules, house, t.kind).len();
            if count == 0 {
                return;
            }
            let h = &world.houses[usize::try_from(house).unwrap_or(0)];
            let time = build_time(
                rules,
                t,
                h.power_output,
                h.power_drain,
                i32::try_from(count).unwrap_or(1),
            );
            let object = world.spawn(rules, techno, house, Coord::default());
            if let Some(o) = world.objects.iter_mut().find(|o| o.id == object) {
                o.limbo = true;
            }
            world.factories.push(Factory {
                house,
                kind: t.kind,
                techno,
                object,
                start: frame,
                rate: (time / STEPS).max(1),
                stage: 0,
                owed: t.cost,
                exit_at: None,
            });
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
            let f = world.factories.remove(k);
            let c = centre(rules, techno, (i32::from(cell.0), i32::from(cell.1)), 0);
            if let Some(o) = world.objects.iter_mut().find(|o| o.id == f.object) {
                o.limbo = false;
                o.coord = c;
                o.placed = frame;
            }
        }
    }
}

/// One frame of production, deploying, units leaving factories, and power.
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    deploy(world, rules, frame);
    for k in 0..world.factories.len() {
        let f = world.factories[k].clone();
        if f.stage < STEPS && frame == f.start + u32::try_from(f.rate * (f.stage + 1)).unwrap_or(0)
        {
            let stage = f.stage + 1;
            let f = &mut world.factories[k];
            f.stage = stage;
            if stage < STEPS {
                // What's still owed over the payments left. `[observed]`
                let pay = f.owed / (STEPS - stage);
                f.owed -= pay;
                if let Some(h) = world
                    .houses
                    .get_mut(usize::try_from(f.house).unwrap_or(usize::MAX))
                {
                    h.credits -= pay;
                }
            } else if f.kind != Kind::Building {
                f.exit_at = Some(frame + 1);
            }
        }
    }
    let leaving: Vec<usize> = (0..world.factories.len())
        .filter(|&k| world.factories[k].exit_at == Some(frame))
        .collect();
    for &k in leaving.iter().rev() {
        let f = world.factories.remove(k);
        leave(world, rules, frame, &f);
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

/// A finished unit comes out of its house's oldest factory of its kind.
fn leave(world: &mut World, rules: &Rules, frame: u32, f: &Factory) {
    let Some(&fi) = factories_of(world, rules, f.house, f.kind).first() else {
        return;
    };
    let (x0, y0) = top_left(rules, &world.objects[fi]);
    let (w, h) = rules.technos[world.objects[fi].techno].foundation;
    let at = Coord {
        x: x0 * LEPTONS_PER_CELL + INFANTRY_EXIT.0,
        y: y0 * LEPTONS_PER_CELL + INFANTRY_EXIT.1,
        z: 0,
    };
    // The first cell of the row below the factory with nobody standing or
    // heading there. `[observed]` (three GIs: (38,39), (39,39), (40,39))
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
    if let Some(o) = world.objects.iter_mut().find(|o| o.id == f.object) {
        o.limbo = false;
        o.coord = at;
        o.placed = frame;
        o.motion.goal = Some(goal);
    }
}

fn cell_i16(v: i32) -> i16 {
    i16::try_from(v).unwrap_or(0)
}

/// Units that just came out walk to their spot, setting off the frame after
/// they appear. `[observed]`
fn walk_out(world: &mut World, rules: &Rules, frame: u32) {
    for o in &mut world.objects {
        if o.limbo || o.kind != Kind::Infantry || o.mission != crate::world::Mission::Guard {
            continue;
        }
        let Some(goal) = o.motion.goal else {
            continue;
        };
        if frame < o.placed + 2 {
            continue;
        }
        if o.motion.leg.is_none() {
            if o.coord == goal {
                o.motion.goal = None;
                continue;
            }
            o.motion.leg = Some((o.coord, goal, 0));
        }
        let speed = rules.technos[o.techno].speed_fraction * WALK_SPEED;
        if travel(o, speed) {
            o.motion.goal = None;
        }
    }
}

/// Each house's power: its buildings' `Power=` outputs, from the frame
/// after they're placed, and drains, from the frame they're placed.
/// `[observed]` (full health only so far)
fn power(world: &mut World, rules: &Rules, frame: u32) {
    for h in &mut world.houses {
        h.power_output = 0;
        h.power_drain = 0;
    }
    for o in &world.objects {
        if o.limbo || o.kind != Kind::Building {
            continue;
        }
        let p = rules.technos[o.techno].power;
        let Some(h) = world
            .houses
            .get_mut(usize::try_from(o.owner).unwrap_or(usize::MAX))
        else {
            continue;
        };
        if p > 0 && frame > o.placed {
            h.power_output += p;
        } else if p < 0 {
            h.power_drain -= p;
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
}
