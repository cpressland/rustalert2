//! Playing a scenario in our engine, writing the same log the observer
//! writes in the original. See `docs/spec/parity.md`.

use ra2_formats::ini::Ini;
use ra2_rules::Rules;
use ra2_rules::scenario::{Placement, placements};
use ra2_sim::tick::Sim;
use ra2_sim::world::World;

use crate::log::{Frame, House, Log, Object};
use crate::scenario::Scenario;

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
    world.add_house(HUMAN.0, HUMAN.1);
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
    for ((country, _), start) in [(HUMAN, human), (AI, ai)] {
        // The base unit by side: the first for the Allies, the second for
        // the Soviets.
        let side = rules
            .countries
            .iter()
            .find(|c| c.id == country)
            .map_or(0, |c| usize::from(c.side.eq_ignore_ascii_case("Nod")));
        let Some(unit) = rules.general.base_units.get(side) else {
            continue;
        };
        let cell = |v: i32| i16::try_from(v).unwrap_or(0);
        world.place(
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
            },
            0,
        );
    }
    // Generated maps are flat, so everything stands at level 0.
    for p in placements(map) {
        world.place(rules, &p, 0);
    }
    world
}

/// What the world holds, as a log frame.
pub fn observe(sim: &Sim, rules: &Rules) -> Frame {
    let world = sim.world();
    Frame {
        frame: sim.frame(),
        houses: world
            .houses
            .iter()
            .map(|h| House {
                index: h.index,
                country: h.country.clone(),
                credits: h.credits,
                power_output: 0,
                power_drain: 0,
                defeated: false,
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
                x: o.coord.x,
                y: o.coord.y,
                z: o.coord.z,
                hp: o.health,
            })
            .collect(),
    }
}

/// Play `scenario` to its end frame, observing every frame from 1.
pub fn run(scenario: &Scenario, rules: &Rules, map: &Ini, seed: u32) -> Log {
    let mut sim = Sim::with_world(seed, setup(scenario, rules, map));
    let mut frames = Vec::new();
    while sim.frame() < scenario.end_frame {
        sim.step(&[]);
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
