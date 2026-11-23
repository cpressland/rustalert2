//! Playing a scenario in our engine, writing the same log the observer
//! writes in the original. See `docs/spec/parity.md`.

use ra2_formats::ini::Ini;
use ra2_rules::Rules;
use ra2_rules::scenario::{Placement, placements};
use ra2_sim::tick::{Order, Sim};
use ra2_sim::world::{Ore, World};

use crate::log::{Frame, House, Log, Object};
use crate::scenario::Scenario;

/// Find the install, mount it, and build the rules (with the scenario
/// map's sections loaded over `rulesmd.ini`, as the game does) and the map.
///
/// # Errors
/// If there's no install or a file is missing.
pub fn prepare(
    scenario: &Scenario,
    game_dir: Option<&std::path::Path>,
) -> std::io::Result<(ra2_vfs::mount::Vfs, Rules, Ini)> {
    use ra2_vfs::install::{Game, Inspection, SearchContext};
    use std::io;
    let inspection = match game_dir {
        Some(d) => Inspection::of(d)?,
        None => {
            SearchContext::from_env()
                .find(Game::YurisRevenge)
                .map_err(io::Error::other)?
                .inspection
        }
    };
    let mut vfs = ra2_vfs::mount::Vfs::new(inspection);
    vfs.mount_startup(Game::YurisRevenge);
    let read = |n: &str| -> io::Result<Ini> {
        let d = vfs
            .read(n)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{n}: not found")))?;
        Ok(Ini::parse(&d))
    };
    let map = match &scenario.generate {
        Some(spec) => Ini::parse(crate::mapgen::write(spec).as_bytes()),
        None => read(&scenario.map)?,
    };
    let mut rules_ini = read("rulesmd.ini")?;
    for section in map.sections() {
        for (k, v) in section.entries() {
            rules_ini.put(&section.name, k, v);
        }
    }
    let rules = Rules::from_ini(&rules_ini, &read("artmd.ini")?);
    Ok((vfs, rules, map))
}

/// The skirmish the recorder starts: the human (British, 10,000 credits)
/// at `human_start`, one AI (Russians, 50,000) at the next start point,
/// then the map's own houses `Neutral` and `Special`. `[observed]` (every
/// reference log so far)
const HUMAN: (&str, i32) = ("British", 10_000);
const AI: (&str, i32) = ("Russians", 50_000);

/// Set up `scenario`'s world: houses, each player's base unit at their
/// start, then the map's objects. `map` is the scenario's map text.
pub fn setup(scenario: &Scenario, rules: &Rules, map: &Ini) -> World {
    let mut world = World::default();
    let human = (
        scenario.human_country.as_deref().unwrap_or(HUMAN.0),
        scenario.credits.unwrap_or(HUMAN.1),
    );
    world.add_house(human.0, human.1);
    world.add_house(AI.0, AI.1);
    world.add_house("Neutral", 0);
    world.add_house("Special", 0);
    let starts: Vec<[i32; 2]> = scenario
        .generate
        .as_ref()
        .map(|g| g.starts.clone())
        .unwrap_or_default();
    let human = scenario
        .human_start
        .or_else(|| starts.first().copied())
        .unwrap_or([0, 0]);
    let ai = starts
        .iter()
        .copied()
        .find(|s| *s != human)
        .unwrap_or(human);
    let human_country = scenario.human_country.as_deref().unwrap_or(HUMAN.0);
    for (index, (country, start)) in [(human_country, human), (AI.0, ai)].into_iter().enumerate() {
        // The base unit by side: Allies, Soviets, Yuri.
        let side = rules
            .countries
            .iter()
            .find(|c| c.id == country)
            .map_or(0, |c| match c.side.to_ascii_lowercase().as_str() {
                "nod" => 1,
                "thirdside" => 2,
                _ => 0,
            });
        let Some(unit) = rules.general.base_units.get(side) else {
            continue;
        };
        let cell = |v: i32| i16::try_from(v).unwrap_or(0);
        // Placed for its house by index: the human and the AI may share a
        // country.
        let placed = world.place(
            rules,
            &Placement {
                kind: ra2_rules::Kind::Vehicle,
                type_id: unit.clone(),
                owner: country.to_string(),
                cell: (cell(start[0]), cell(start[1])),
                sub_cell: 0,
                health: 256,
                facing: 0,
                mission: "Guard".into(),
                veterancy: 0,
            },
            0,
        );
        if placed.is_some()
            && let Some(o) = world.objects.last_mut()
        {
            o.owner = u32::try_from(index).unwrap_or(0);
        }
    }
    // The map's cells, for route planning.
    if let Ok(m) = ra2_formats::map::Map::from_ini(map) {
        world.cells = m.tiles.iter().map(|t| (t.x, t.y)).collect();
        // Ore and gems, from the overlay packs (`y * 512 + x`).
        if let (Some(overlay), Some(data)) = (&m.overlay, &m.overlay_data) {
            for (idx, (&o, &d)) in overlay.iter().zip(data).enumerate() {
                if o != 0xFF && Ore::is_ore(o) {
                    let (x, y) = (idx % 512, idx / 512);
                    if let (Ok(x), Ok(y)) = (i16::try_from(x), i16::try_from(y)) {
                        world.ore.insert(
                            (x, y),
                            Ore {
                                overlay: o,
                                data: d,
                            },
                        );
                    }
                }
            }
        }
    }
    // Generated maps are flat, so everything stands at level 0.
    for p in placements(map) {
        world.place(rules, &p, 0);
    }
    world
}

/// The game's `AbstractType` of a type list, as production and placing
/// events name what they build. `[recomp 0x006AB4FF]` (the sidebar's
/// cameo; the four values are those the per-kind multiplier switches on,
/// `0x0050C0A0`; placing names the
/// object's kind, one less)
fn type_rtti(kind: ra2_rules::Kind) -> i32 {
    match kind {
        ra2_rules::Kind::Aircraft => 3,
        ra2_rules::Kind::Building => 7,
        ra2_rules::Kind::Infantry => 16,
        ra2_rules::Kind::Vehicle => 40,
    }
}

/// A type's position in its list (`[BuildingTypes]` and so on), as the
/// game numbers its type arrays.
fn type_index(rules: &Rules, id: &str) -> Option<(ra2_rules::Kind, usize)> {
    let t = rules
        .technos
        .iter()
        .position(|t| t.id.eq_ignore_ascii_case(id))?;
    let kind = rules.technos[t].kind;
    let index = rules.technos[..t].iter().filter(|o| o.kind == kind).count();
    Some((kind, index))
}

/// A scenario's orders as the observer reads them (`RA2_OBSERVE_ORDERS`,
/// see `parity/recomp/observe.c`). Orders naming unknown types are left
/// out.
pub fn observer_orders(scenario: &Scenario, rules: &Rules) -> String {
    use crate::scenario::OrderKind;
    scenario
        .orders
        .iter()
        .filter_map(|o| {
            Some(match o.what {
                OrderKind::Deploy => format!("{} D {}", o.frame, o.type_id),
                OrderKind::Sell => format!("{} S {}", o.frame, o.type_id),
                OrderKind::Repair => format!("{} R {}", o.frame, o.type_id),
                OrderKind::Abandon => {
                    let (kind, index) = type_index(rules, &o.type_id)?;
                    format!("{} A {} {index} 0", o.frame, type_rtti(kind))
                }
                OrderKind::Hold => {
                    let (kind, index) = type_index(rules, &o.type_id)?;
                    format!("{} H {} {index} 0", o.frame, type_rtti(kind))
                }
                OrderKind::Fire => {
                    let sw = rules
                        .super_weapons
                        .iter()
                        .position(|s| s.id.eq_ignore_ascii_case(&o.type_id))?;
                    format!("{} F {sw} {} {}", o.frame, o.x, o.y)
                }
                OrderKind::Produce => {
                    let (kind, index) = type_index(rules, &o.type_id)?;
                    format!("{} P {} {index} 0", o.frame, type_rtti(kind))
                }
                OrderKind::Place => {
                    let (kind, index) = type_index(rules, &o.type_id)?;
                    format!(
                        "{} L {} {index} 0 {} {}",
                        o.frame,
                        type_rtti(kind) - 1,
                        o.x,
                        o.y
                    )
                }
            })
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// What the world holds, as a log frame.
pub fn observe(sim: &Sim, rules: &Rules) -> Frame {
    let world = sim.world();
    let logged = |o: &ra2_sim::world::Object| {
        if o.limbo {
            (0, 0)
        } else if o.kind == ra2_rules::Kind::Building {
            let (x, y) = ra2_sim::production::top_left(rules, o);
            (x * 256 + 128, y * 256 + 128)
        } else {
            (o.coord.x, o.coord.y)
        }
    };
    Frame {
        frame: sim.frame(),
        houses: world
            .houses
            .iter()
            .map(|h| House {
                index: h.index,
                country: h.country.clone(),
                credits: h.credits,
                power_output: h.power_output,
                power_drain: h.power_drain,
                defeated: false,
                supers: {
                    let mut v: Vec<[i32; 4]> = world
                        .supers
                        .iter()
                        .filter(|s| s.house == h.index)
                        .map(|s| {
                            [
                                i32::try_from(s.kind).unwrap_or(-1),
                                i32::from(s.charged),
                                i32::from(s.on_hold),
                                s.left(sim.frame()),
                            ]
                        })
                        .collect();
                    v.sort_unstable();
                    v
                },
            })
            .collect(),
        objects: world
            .objects
            .iter()
            .map(|o| Object {
                id: o.id,
                kind: match o.kind {
                    ra2_rules::Kind::Building => "Building",
                    ra2_rules::Kind::Vehicle => "Unit",
                    ra2_rules::Kind::Infantry => "Infantry",
                    ra2_rules::Kind::Aircraft => "Aircraft",
                }
                .to_string(),
                type_id: rules.technos[o.techno].id.clone(),
                owner: o.owner,
                // The game logs a building at its top-left cell's centre, and
                // anything in limbo at 0.
                x: logged(o).0,
                y: logged(o).1,
                // Aircraft in limbo sit at the flight level. `[observed]`
                z: match (o.limbo, o.kind) {
                    (true, ra2_rules::Kind::Aircraft) => rules.general.flight_level,
                    (true, _) => 0,
                    _ => o.coord.z,
                },
                hp: o.health,
                cargo: o.cargo,
                #[allow(clippy::cast_possible_truncation)]
                vet_micros: (f64::from(f32::from_bits(o.experience_bits)) * 1e6).round() as i64,
            })
            .collect(),
        ore: world
            .ore
            .iter()
            .map(|(&(x, y), o)| {
                [
                    i32::from(x),
                    i32::from(y),
                    i32::from(o.overlay),
                    i32::from(o.data),
                ]
            })
            .collect(),
    }
}

/// The scenario's orders for the human player (house 0), by the frame
/// they're sent on: they take effect on the next, as the original executes
/// the events.
pub fn sim_orders(scenario: &Scenario, rules: &Rules) -> Vec<(u32, Order)> {
    use crate::scenario::OrderKind;
    use ra2_sim::production::Build;
    scenario
        .orders
        .iter()
        .filter_map(|o| {
            let house = 0;
            let cell = (i16::try_from(o.x).ok()?, i16::try_from(o.y).ok()?);
            if o.what == OrderKind::Fire {
                let kind = rules
                    .super_weapons
                    .iter()
                    .position(|s| s.id.eq_ignore_ascii_case(&o.type_id))?;
                return Some((o.frame, Order::Fire { house, kind, cell }));
            }
            let techno = rules
                .technos
                .iter()
                .position(|t| t.id.eq_ignore_ascii_case(&o.type_id))?;
            let b = match o.what {
                OrderKind::Deploy => Build::Deploy { house, techno },
                OrderKind::Produce => Build::Produce { house, techno },
                OrderKind::Place => Build::Place {
                    house,
                    techno,
                    cell,
                },
                OrderKind::Sell => Build::Sell { house, techno },
                OrderKind::Repair => Build::Repair { house, techno },
                OrderKind::Abandon => Build::Abandon { house, techno },
                OrderKind::Hold => Build::Hold { house, techno },
                OrderKind::Fire => return None,
            };
            Some((o.frame, Order::Build(b)))
        })
        .collect()
}

/// Play `scenario` to its end frame, observing every frame from 1.
pub fn run(scenario: &Scenario, rules: &Rules, map: &Ini, seed: u32) -> Log {
    let mut sim = Sim::start(seed, rules.clone(), setup(scenario, rules, map));
    let mut frames = Vec::new();
    let orders = sim_orders(scenario, rules);
    while sim.frame() < scenario.end_frame {
        let now: Vec<Order> = orders
            .iter()
            .filter(|(f, _)| *f == sim.frame())
            .map(|(_, o)| o.clone())
            .collect();
        sim.step(&now);
        frames.push(observe(&sim, rules));
    }
    Log { frames }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generated_scenario_starts_like_the_original() {
        let scenario = Scenario::parse(
            "name='t'\nmap='t.yrm'\nend_frame=3\nhuman_start=[35,35]\n\
             [generate]\nname='t'\nwidth=50\nheight=50\nstarts=[[35,35],[65,65]]\n\
             [[generate.house]]\nname='Neutral'\n\
             [[generate.unit]]\nowner='Neutral'\ntype='MTNK'\nx=47\ny=50\nfacing=64\nmission='Hunt'\n",
        )
        .unwrap();
        let rules = Rules::from_ini(
            &Ini::parse(
                b"[General]\nBaseUnit=AMCV,SMCV\n[VehicleTypes]\n1=AMCV\n2=SMCV\n3=MTNK\n\
                  [Countries]\n1=British\n2=Russians\n[British]\nSide=GDI\n[Russians]\nSide=Nod\n\
                  [AMCV]\nStrength=1000\n[SMCV]\nStrength=1000\n[MTNK]\nStrength=300\n",
            ),
            &Ini::parse(b""),
        );
        let map = Ini::parse(crate::mapgen::write(scenario.generate.as_ref().unwrap()).as_bytes());
        let log = run(&scenario, &rules, &map, 1);
        let f = log.at(1).unwrap();
        let summary: Vec<_> = f
            .objects
            .iter()
            .map(|o| (o.type_id.as_str(), o.owner, o.x, o.y, o.hp))
            .collect();
        // As in the reference log of grizzly-hunts-mcv at frame 1.
        assert_eq!(
            summary,
            [
                ("AMCV", 0, 9088, 9088, 1000),
                ("SMCV", 1, 16768, 16768, 1000),
                ("MTNK", 2, 12160, 12928, 300)
            ]
        );
        assert_eq!(log.at(3).unwrap().frame, 3);
    }
}

#[cfg(test)]
mod hunt_tests {
    use super::*;

    /// The grizzly-hunts-mcv scenario with its rules values written out, so
    /// CI (which has no game data) checks what the reference log shows:
    /// the Grizzly sets off on frame 20, stops at (38,39) on frame 159 and
    /// its first shell lands on frame 170, then every 60–62 frames.
    #[test]
    fn a_grizzly_hunts_an_mcv_as_in_the_reference_log() {
        let scenario = Scenario::parse(
            "name='t'\nmap='t.yrm'\nend_frame=300\nhuman_start=[35,35]\n\
             [generate]\nname='t'\nwidth=50\nheight=50\nstarts=[[35,35],[65,65]]\n\
             [[generate.house]]\nname='Neutral'\n\
             [[generate.unit]]\nowner='Neutral'\ntype='MTNK'\nx=47\ny=50\nfacing=64\nmission='Hunt'\n",
        )
        .unwrap();
        let rules = Rules::from_ini(
            &Ini::parse(
                b"[General]\nBaseUnit=AMCV,SMCV\n[VehicleTypes]\n1=AMCV\n2=SMCV\n3=MTNK\n\
                  [Warheads]\n1=AP\n\
                  [Countries]\n1=British\n2=Russians\n[British]\nSide=GDI\n[Russians]\nSide=Nod\n\
                  [AMCV]\nStrength=1000\nArmor=heavy\n[SMCV]\nStrength=1000\nArmor=heavy\n\
                  [MTNK]\nStrength=300\nArmor=heavy\nSpeed=7\nROT=5\nAccelerates=no\nPrimary=105mm\nTurret=yes\n\
                  [105mm]\nDamage=65\nROF=60\nRange=5\nProjectile=Cannon\nSpeed=40\nWarhead=AP\n\
                  [Cannon]\nArcing=true\n\
                  [AP]\nVerses=25%,25%,15%,75%,100%,100%,65%,45%,60%,60%,100%\n",
            ),
            &Ini::parse(b""),
        );
        let map = Ini::parse(crate::mapgen::write(scenario.generate.as_ref().unwrap()).as_bytes());
        let log = run(&scenario, &rules, &map, 1);
        let tank = |f: u32| {
            let o = log
                .at(f)
                .unwrap()
                .objects
                .iter()
                .find(|o| o.type_id == "MTNK")
                .cloned()
                .unwrap();
            (o.x, o.y)
        };
        let mcv = |f: u32| {
            log.at(f)
                .unwrap()
                .objects
                .iter()
                .find(|o| o.type_id == "AMCV")
                .unwrap()
                .hp
        };
        assert_eq!(tank(19), (12160, 12928), "still turning");
        assert_ne!(tank(20), tank(19), "sets off on frame 20");
        assert_eq!(tank(159), (9856, 10112), "stops at (38,39)");
        assert_eq!(tank(300), tank(159));
        assert_eq!((mcv(169), mcv(170)), (1000, 935), "first hit on frame 170");
        let hits = (171..=300)
            .filter(|&f| mcv(f) < mcv(f - 1))
            .collect::<Vec<_>>();
        assert!(
            hits.len() == 2 && (230..=232).contains(&hits[0]),
            "{hits:?}"
        );
    }
}
