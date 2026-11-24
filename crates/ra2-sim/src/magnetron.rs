//! The Magnetron's beam (`IsLocomotor=` warheads): what it hits is lifted
//! and carried as a jumpjet, by `[JumpjetControls]`, to a cell beside the
//! Magnetron, and dropped. See `docs/spec/abilities.md`, "The Magnetron".
//!
//! Recorded (`yuri-magnetron`, a Grizzly lifted from 8 cells): it rises
//! `Climb=` leptons a frame from 2 frames after the beam hits, to
//! `CruiseHeight=`; once above half that it sets off toward the cell,
//! gaining `Acceleration=` a frame while that keeps it under `Speed=`. At
//! the cruise height it wobbles by `WobbleDeviation=`, `WobblesPerSecond=`
//! times a second (15 frames). Near the cell it slows and sinks, then drops
//! onto the cell's centre, falling 10 leptons a frame and one more each
//! frame (at most 28), sinks below the ground by its last step and comes
//! back up 5 a frame. It isn't hurt, and comes down awake. `[observed]`
//!
//! The approach (where it starts, and its speeds) is fitted; the jumpjet's
//! states (`JumpjetLocomotionClass`, Process at `0x0054B03C`) aren't
//! traced through.

use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::hunt::{distance, facing_to};
use crate::world::{Mission, World};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lift {
    /// The Magnetron holding it.
    pub by: u32,
    /// Where it was lifted from, the centre of the cell it's carried to,
    /// and how far along the ground it's gone.
    pub from: Coord,
    pub to: Coord,
    pub gone: i32,
    pub phase: Phase,
    /// The frame it starts rising.
    pub rises: u32,
    /// The frame it reached the cruise height, for the wobble.
    pub cruising: Option<u32>,
    /// Leptons a frame, along the ground and falling.
    pub speed: i32,
    pub fall: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Rise,
    /// Near the cell: slowing and sinking; how many frames in.
    Approach(u32),
    Fall,
    Settle,
}

/// Frames from the beam hitting to the first rise. `[observed]`
const RISE_DELAY: u32 = 2;
/// How near the cell it starts its approach, in leptons. Fitted (172 did,
/// 185 didn't).
const APPROACH: i32 = 180;
/// Its approach: slowing by 3 a frame for three frames, then 6 a frame,
/// sinking 5. Fitted.
const APPROACH_SLOWING: u32 = 3;
const APPROACH_SPEED: i32 = 6;
const SINK: i32 = 5;
/// Its fall: 10 a frame, one more each frame, at most 28; then back up 5
/// a frame. `[observed]`
const FALL_START: i32 = 10;
const FALL_MAX: i32 = 28;
const SETTLE: i32 = 5;
/// Frames a second, for `WobblesPerSecond=`.
const FRAMES_PER_SECOND: i32 = 15;

/// The beam from Magnetron `by` hits `target`: a vehicle not already held
/// is lifted, to be carried to the cell beside the Magnetron 45° counter-
/// clockwise of the way to it. `[hypothesis]` (one lift: from due west, to
/// the cell south-west)
pub(crate) fn grab(world: &mut World, frame: u32, by: u32, target: u32) {
    let Some(from) = world.objects.iter().find(|o| o.id == by).map(|o| o.coord) else {
        return;
    };
    let Some(o) = world.objects.iter_mut().find(|o| o.id == target) else {
        return;
    };
    if o.kind != Kind::Vehicle || o.lift.is_some() {
        return;
    }
    let way = facing_to(from, o.coord);
    let side = (way - 8192).rem_euclid(65_536);
    // The nearest of the eight directions.
    let k = ((side + 4096) / 8192) % 8;
    let (dx, dy) = [
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
    ][usize::try_from(k).unwrap_or(0)];
    let cell = (
        from.x / LEPTONS_PER_CELL + dx,
        from.y / LEPTONS_PER_CELL + dy,
    );
    let start = o.coord;
    o.lift = Some(Lift {
        by,
        from: start,
        gone: 0,
        to: Coord {
            x: cell.0 * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            y: cell.1 * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
            z: 0,
        },
        phase: Phase::Rise,
        rises: frame + RISE_DELAY,
        cruising: None,
        speed: -1,
        fall: FALL_START,
    });
    o.command = None;
    o.motion = crate::hunt::Motion::default();
}

/// Carry `c` `step` leptons further along the ground on its line from
/// `lift.from` to `lift.to`.
fn toward(c: &mut Coord, lift: &mut Lift, step: i32) {
    let flat = |c: Coord| Coord { z: 0, ..c };
    let total = distance(flat(lift.from), flat(lift.to));
    lift.gone = (lift.gone + step.max(0)).min(total);
    if total == 0 || lift.gone == total {
        c.x = lift.to.x;
        c.y = lift.to.y;
    } else {
        c.x = lift.from.x + (lift.to.x - lift.from.x) * lift.gone / total;
        c.y = lift.from.y + (lift.to.y - lift.from.y) * lift.gone / total;
    }
}

/// One frame of everything held up.
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    let j = &rules.jumpjet;
    for o in &mut world.objects {
        let Some(mut lift) = o.lift.clone() else {
            continue;
        };
        if frame < lift.rises {
            continue;
        }
        let flat = |c: Coord| Coord { z: 0, ..c };
        match lift.phase {
            Phase::Rise => {
                match lift.cruising {
                    None => {
                        o.coord.z = (o.coord.z + j.climb).min(j.cruise_height);
                        if o.coord.z >= j.cruise_height {
                            lift.cruising = Some(frame);
                        }
                    }
                    Some(since) => {
                        let per_frame = j.wobbles_per_second.of(65_536) / FRAMES_PER_SECOND;
                        let elapsed = i32::try_from(frame - since).unwrap_or(0);
                        let angle = (elapsed * per_frame).rem_euclid(65_536);
                        // The east part of a step along `angle` is its sine.
                        let (s, _) = crate::airstrike::ahead(angle, j.wobble_deviation);
                        o.coord.z = j.cruise_height + s;
                    }
                }
                if o.coord.z > j.cruise_height / 2 {
                    if lift.speed + j.acceleration < j.speed {
                        lift.speed += j.acceleration;
                    }
                    if distance(flat(o.coord), flat(lift.to)) < APPROACH {
                        lift.phase = Phase::Approach(0);
                    } else {
                        let step = lift.speed;
                        toward(&mut o.coord, &mut lift, step);
                    }
                }
                if let Phase::Approach(_) = lift.phase {
                    // Its first slowing step, this frame.
                    lift.speed -= 3;
                    let step = lift.speed;
                    toward(&mut o.coord, &mut lift, step);
                    lift.phase = Phase::Approach(1);
                }
            }
            Phase::Approach(n) => {
                lift.speed = if n < APPROACH_SLOWING {
                    lift.speed - 3
                } else {
                    APPROACH_SPEED
                };
                o.coord.z -= SINK;
                let step = lift.speed;
                toward(&mut o.coord, &mut lift, step);
                lift.phase = if flat(o.coord) == flat(lift.to) {
                    Phase::Fall
                } else {
                    Phase::Approach(n + 1)
                };
            }
            Phase::Fall => {
                o.coord.z -= lift.fall;
                lift.fall = (lift.fall + 1).min(FALL_MAX);
                if o.coord.z <= 0 {
                    lift.phase = Phase::Settle;
                }
            }
            Phase::Settle => {
                o.coord.z = (o.coord.z + SETTLE).min(0);
                if o.coord.z == 0 {
                    // Down, awake: it fights back.
                    o.lift = None;
                    o.mission = Mission::Guard;
                    o.retaliate = Some(lift.by);
                    continue;
                }
            }
        }
        o.lift = Some(lift);
    }
}
