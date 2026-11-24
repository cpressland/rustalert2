//! Airstrikes: Boris's flare (`Airstrike=` warheads) calling in planes. See
//! `docs/spec/abilities.md`, "Airstrikes".
//!
//! What's recorded: the planes come in pairs (`AirstrikeTeam=`) on the frame
//! after the flare, on a cell of the map's top edge chosen at random, at
//! `FlightLevel=`, facing north. They stand 3 frames, then gain 4 leptons a
//! frame to their speed; each fires its weapon's burst at the target once
//! (`Ammo=1`) and flies back off the top edge. The caller can call again
//! `AirstrikeRechargeTime=` frames after the last has gone, with its next
//! flare. `[observed]` (`soviet-boris-airstrike`, six seeds)
//!
//! How they fly between is fitted, not traced: they turn toward where
//! they're going at `ROT=` × 256 a frame and fire once within their weapon's
//! range and pointed at the target. The game's flying locomotor and its
//! aircraft attack mission (strafing passes, loops) aren't traced, nor how
//! the game picks the cell they come in on; together these put the first
//! hit 1380 to 1481 frames in over six recordings, ours 1555.

use ra2_rules::{Kind, Rules};

use crate::coords::{Coord, LEPTONS_PER_CELL};
use crate::hunt::{distance, facing_to, turn};
use crate::rng::Randomizer;
use crate::world::{Mission, Shot, World};

/// The map's top edge, where airstrike planes come and go: the cells with
/// `x + y = sum` and `x - y` from `min` to `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub sum: i32,
    pub min: i32,
    pub max: i32,
}

impl Edge {
    /// The top edge of a map of `width` cells whose visible part is
    /// `local` (`LocalSize=`: x, y, width, height). `[observed]` (every
    /// plane in and out, on a 50×50 map with `LocalSize=2,4,46,42`, was on
    /// a cell with x + y = 59 and x − y from −39 to 45)
    pub fn top(width: i32, local: (i32, i32, i32, i32)) -> Edge {
        let (lx, ly, lw, _) = local;
        Edge {
            sum: width + 1 + 2 * ly,
            min: 2 * lx - width + 1,
            max: 2 * (lx + lw) - width + 1,
        }
    }

    /// The cell of the edge at `x - y = diff`, moved onto the edge's row.
    fn cell(self, diff: i32) -> (i32, i32) {
        let mut d = diff.clamp(self.min, self.max);
        if (self.sum - d).rem_euclid(2) != 0 {
            d += if d < self.max { 1 } else { -1 };
        }
        (i32::midpoint(self.sum, d), i32::midpoint(self.sum, -d))
    }
}

/// One caller's airstrike: its planes in the air, and when it can call
/// again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Airstrike {
    pub caller: u32,
    pub target: u32,
    pub planes: Vec<Plane>,
    /// The frame from which a flare calls the next.
    pub ready_at: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plane {
    pub id: u32,
    /// The frame it came in.
    pub entered: u32,
    /// Leptons a frame.
    pub speed: i32,
    /// Shots of its burst still to fire, and the frame of the next.
    pub shots: i32,
    pub next_shot: u32,
    pub fired: bool,
}

/// Frames a plane stands after coming in before it moves, and what it
/// gains a frame after. `[observed]`
const STILL_FRAMES: u32 = 3;
const ACCELERATION: i32 = 4;
/// Frames between the shots of a plane's burst. Fitted (the hits of a
/// pair's four missiles came within 6 frames)
const BURST_GAP: u32 = 2;
/// How far off the target a plane may point and still fire, in 65,536ths
/// of a turn (45°). Fitted.
const AIM: i32 = 8192;
/// A missile's speed, leptons a frame (`Maverick3`'s `Speed=70`).
const MISSILE_SPEED: i32 = 70;

/// A flare from `caller` landing on `target`: it calls the caller's planes
/// if none are in the air and it has recharged. They come in on the next
/// frame.
pub(crate) fn call(
    world: &mut World,
    rules: &Rules,
    rng: &mut Randomizer,
    frame: u32,
    caller: usize,
    target: u32,
) {
    let c = &world.objects[caller];
    let t = &rules.technos[c.techno];
    let elite = c.rank() >= 2;
    let (count, kind) = if elite {
        (
            t.elite_airstrike_team,
            t.elite_airstrike_team_type.as_deref(),
        )
    } else {
        (t.airstrike_team, t.airstrike_team_type.as_deref())
    };
    let Some(plane) = kind.and_then(|k| rules.techno_index(k)) else {
        return;
    };
    let (id, owner) = (c.id, c.owner);
    if world
        .airstrikes
        .iter()
        .any(|a| a.caller == id && (!a.planes.is_empty() || frame < a.ready_at))
    {
        return;
    }
    let Some(target_at) = world
        .objects
        .iter()
        .find(|o| o.id == target)
        .map(|o| o.coord)
    else {
        return;
    };
    // A cell of the top edge, at random; without one (a map we don't know
    // the size of), 16 cells north of the target.
    let cell = if let Some(e) = world.top_edge {
        e.cell(rng.ranged(e.min, e.max))
    } else {
        (
            target_at.x / LEPTONS_PER_CELL - 16,
            target_at.y / LEPTONS_PER_CELL - 16,
        )
    };
    let at = Coord {
        x: cell.0 * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        y: cell.1 * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
        z: rules.general.flight_level,
    };
    let burst = rules.technos[plane]
        .primary
        .map_or(1, |w| rules.weapon(w).burst.max(1));
    let mut planes = Vec::new();
    for _ in 0..count.max(0) {
        let p = world.spawn(rules, plane, owner, at);
        if let Some(o) = world.objects.iter_mut().find(|o| o.id == p) {
            o.mission = Mission::Other;
            o.dir = 0;
            o.facing = 0;
        }
        planes.push(Plane {
            id: p,
            entered: frame,
            speed: 0,
            shots: burst,
            next_shot: 0,
            fired: false,
        });
    }
    world.airstrikes.retain(|a| a.caller != id);
    world.airstrikes.push(Airstrike {
        caller: id,
        target,
        planes,
        ready_at: 0,
    });
}

/// The step `len` leptons long along a facing (65,536ths of a turn,
/// clockwise from north), by CORDIC rotation.
fn ahead(dir: i32, len: i32) -> (i32, i32) {
    const ATAN: [i64; 15] = [
        8192, 4836, 2555, 1297, 651, 326, 163, 81, 41, 20, 10, 5, 3, 1, 1,
    ];
    // Into −90°..90° of north first.
    let mut a = i64::from(dir.rem_euclid(65_536));
    if a > 32_768 {
        a -= 65_536;
    }
    let flip = a.abs() > 16_384;
    if flip {
        a -= 32_768 * a.signum();
    }
    // (north, east) components, scaled by the CORDIC gain's inverse.
    let (mut n, mut e): (i64, i64) = ((i64::from(len) * 39_797) << 8, 0);
    let mut left = a;
    for (k, step) in ATAN.iter().enumerate() {
        let (nn, ne);
        if left > 0 {
            nn = n - (e >> k);
            ne = e + (n >> k);
            left -= step;
        } else {
            nn = n + (e >> k);
            ne = e - (n >> k);
            left += step;
        }
        n = nn;
        e = ne;
    }
    let scale = |v: i64| i32::try_from(v >> 24).unwrap_or(0);
    let (n, e) = if flip { (-n, -e) } else { (n, e) };
    (scale(e), -scale(n))
}

/// One frame of every airstrike: planes come in, fly, fire and leave.
pub fn update(world: &mut World, rules: &Rules, frame: u32) {
    for k in 0..world.airstrikes.len() {
        let target = world.airstrikes[k].target;
        let target_at = world
            .objects
            .iter()
            .find(|o| o.id == target && o.health > 0)
            .map(|o| o.coord);
        let edge = world.top_edge;
        let mut gone = Vec::new();
        for p in 0..world.airstrikes[k].planes.len() {
            let plane = world.airstrikes[k].planes[p].clone();
            let Some(i) = world.objects.iter().position(|o| o.id == plane.id) else {
                gone.push(p);
                continue;
            };
            if frame <= plane.entered + STILL_FRAMES {
                continue;
            }
            let t = &rules.technos[world.objects[i].techno];
            let top = t.speed * LEPTONS_PER_CELL / 100;
            let rot = t.rot * 256;
            let weapon = t.primary;
            let mut plane = plane;
            plane.speed = (plane.speed + ACCELERATION).min(top);
            let o = &mut world.objects[i];
            let leaving = plane.fired || target_at.is_none();
            let goal = if let (false, Some(at)) = (leaving, target_at) {
                at
            } else {
                // The nearest cell of the top edge.
                let (x, y) = (o.coord.x / LEPTONS_PER_CELL, o.coord.y / LEPTONS_PER_CELL);
                let (cx, cy) = edge.map_or((x - 16, y - 16), |e| e.cell(x - y));
                Coord {
                    x: cx * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
                    y: cy * LEPTONS_PER_CELL + LEPTONS_PER_CELL / 2,
                    z: o.coord.z,
                }
            };
            let flat = |c: Coord| Coord { z: 0, ..c };
            let want = facing_to(flat(o.coord), flat(goal));
            let now = turn(i32::from(o.dir), want, rot);
            o.dir = u16::try_from(now).unwrap_or(0);
            o.facing = u8::try_from(o.dir >> 8).unwrap_or(0);
            let (dx, dy) = ahead(now, plane.speed);
            o.coord.x += dx;
            o.coord.y += dy;
            // Out over the top edge, it's gone.
            if leaving {
                let (x, y) = (o.coord.x / LEPTONS_PER_CELL, o.coord.y / LEPTONS_PER_CELL);
                if edge.is_none_or(|e| x + y <= e.sum) {
                    gone.push(p);
                    world.objects.remove(i);
                    world.airstrikes[k].planes[p] = plane;
                    continue;
                }
            }
            // Within range and pointed at the target, it fires its burst.
            if let (false, Some(at), Some(w)) = (leaving, target_at, weapon) {
                let weapon_t = rules.weapon(w);
                let off = (want - now).rem_euclid(65_536);
                let off = off.min(65_536 - off);
                let near = distance(flat(o.coord), flat(at)) <= weapon_t.range;
                if plane.shots > 0 && near && off <= AIM && frame >= plane.next_shot {
                    let flight = distance(o.coord, at) / MISSILE_SPEED;
                    let shooter = o.id;
                    world.shots.push(Shot {
                        shooter,
                        damage: weapon_t.damage,
                        target,
                        weapon: w,
                        lands: frame + 1 + u32::try_from(flight).unwrap_or(0),
                    });
                    plane.shots -= 1;
                    plane.next_shot = frame + BURST_GAP;
                    if plane.shots == 0 {
                        plane.fired = true;
                    }
                }
            }
            world.airstrikes[k].planes[p] = plane;
        }
        let a = &mut world.airstrikes[k];
        for &p in gone.iter().rev() {
            a.planes.remove(p);
        }
        // The last one gone, the caller recharges.
        if a.planes.is_empty() && a.ready_at == 0 {
            let caller = a.caller;
            let recharge = world
                .objects
                .iter()
                .find(|o| o.id == caller)
                .map_or(0, |o| {
                    let t = &rules.technos[o.techno];
                    if o.rank() >= 2 {
                        t.elite_airstrike_recharge
                    } else {
                        t.airstrike_recharge
                    }
                });
            world.airstrikes[k].ready_at = frame + u32::try_from(recharge).unwrap_or(0);
        }
    }
}

/// Whether a weapon calls in an airstrike rather than hurting.
pub(crate) fn is_flare(rules: &Rules, w: ra2_rules::WeaponId) -> bool {
    rules
        .weapon(w)
        .warhead
        .is_some_and(|wh| rules.warhead(wh).airstrike)
}

/// Whether `o` can call in planes against buildings (a flare on its
/// `Secondary=`).
pub(crate) fn flare_for(rules: &Rules, t: &ra2_rules::TechnoType, target: Kind) -> bool {
    target == Kind::Building && t.secondary.is_some_and(|w| is_flare(rules, w))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_along_the_principal_directions() {
        let close = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1;
        assert!(close(ahead(0, 40), (0, -40)));
        assert!(close(ahead(16_384, 40), (40, 0)));
        assert!(close(ahead(32_768, 40), (0, 40)));
        assert!(close(ahead(49_152, 40), (-40, 0)));
        assert!(close(ahead(8_192, 40), (28, -28)));
    }

    #[test]
    fn the_top_edge_of_a_parity_map() {
        let e = Edge::top(50, (2, 4, 46, 42));
        assert_eq!(e.sum, 59);
        assert_eq!(e.cell(-13), (23, 36));
        assert_eq!(e.cell(37), (48, 11));
    }
}
