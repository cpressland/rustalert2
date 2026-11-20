//! Every scenario in `parity/scenarios/` parses, and its generated map reads
//! back with our own map reader. No game data needed.

use ra2_formats::ini::Ini;
use ra2_formats::map::Map;
use ra2_parity::mapgen;
use ra2_parity::scenario::Scenario;

#[test]
fn shipped_scenarios_are_valid() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../parity/scenarios");
    let mut n = 0;
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "toml") {
            continue;
        }
        let s = Scenario::parse(&std::fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!s.checks.is_empty(), "{}: no checks", s.name);
        assert!(
            path.file_stem().unwrap().to_string_lossy() == s.name,
            "{}: file name and name differ",
            s.name
        );
        if let Some(spec) = &s.generate {
            let ini = Ini::parse(mapgen::write(spec).as_bytes());
            let map = Map::from_ini(&ini).unwrap_or_else(|e| panic!("{}: {e}", s.name));
            assert_eq!(
                map.tiles.len(),
                mapgen::cells(spec.width, spec.height).len()
            );
            for o in spec
                .units
                .iter()
                .chain(&spec.infantry)
                .chain(&spec.structures)
            {
                assert!(
                    mapgen::cells(spec.width, spec.height).contains(&(o.x, o.y)),
                    "{}: {} at {},{} is off the map",
                    s.name,
                    o.type_id,
                    o.x,
                    o.y
                );
                assert!(
                    ["Neutral", "Special"].contains(&o.owner.as_str()),
                    "{}: only Neutral and Special can own objects in a skirmish",
                    s.name
                );
            }
        }
        n += 1;
    }
    assert!(n >= 3, "{n} scenarios");
}
