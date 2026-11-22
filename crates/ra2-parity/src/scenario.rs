//! Scenarios: a map, how long to run it, and what to compare.
//! See `docs/spec/parity.md`.

use serde::Deserialize;

/// A parity scenario, read from TOML.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The scenario's map: a file in `parity/maps/`, or a shipped map's name.
    pub map: String,
    /// The frame to stop at.
    pub end_frame: u32,
    /// A map to write as `map`, if the scenario makes its own.
    #[serde(default)]
    pub generate: Option<crate::mapgen::MapSpec>,
    /// The cell the human player (house 0) must start at. The game assigns
    /// start points at random, so the recorder retries until it matches.
    #[serde(default)]
    pub human_start: Option<[i32; 2]>,
    #[serde(default, rename = "check")]
    pub checks: Vec<Check>,
}

/// What objects a check looks at.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    /// A rules ID, e.g. `MTNK`; any type if absent.
    #[serde(rename = "type", default)]
    pub type_id: Option<String>,
    /// A house index; any owner if absent.
    #[serde(default)]
    pub owner: Option<u32>,
}

/// One comparison.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "what", rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// How many objects match, at a frame.
    Count {
        #[serde(flatten)]
        filter: Filter,
        at: u32,
        #[serde(default)]
        tolerance: u32,
    },
    /// A house's credits at a frame.
    Credits {
        house: u32,
        at: u32,
        #[serde(default)]
        tolerance: u32,
    },
    /// Total hit points of the matching objects at a frame.
    Hp {
        #[serde(flatten)]
        filter: Filter,
        at: u32,
        #[serde(default)]
        tolerance: u32,
    },
    /// Bails carried by the matching objects, in total, at a frame.
    Cargo {
        #[serde(flatten)]
        filter: Filter,
        at: u32,
        #[serde(default)]
        tolerance: u32,
    },
    /// Bails of ore left on the map at a frame: each ore cell's data, summed.
    Ore {
        at: u32,
        #[serde(default)]
        tolerance: u32,
    },
    /// Where the matching objects are at a frame: each object of one log is
    /// paired with the nearest unpaired one of the other, and every pair
    /// must be within the tolerance, in leptons.
    Position {
        #[serde(flatten)]
        filter: Filter,
        at: u32,
        tolerance: u32,
    },
    /// The first frame the matching objects' total hit points drop.
    FirstDamage {
        #[serde(flatten)]
        filter: Filter,
        window: u32,
    },
    /// The first frame a matching object is created or removed.
    Event {
        #[serde(flatten)]
        filter: Filter,
        event: EventKind,
        /// How far apart the two frames may be.
        window: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Created,
    Removed,
}

impl Scenario {
    pub fn parse(text: &str) -> Result<Scenario, toml::de::Error> {
        toml::from_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses() {
        let s = Scenario::parse(
            r#"
name = "grizzly-vs-rhino"
map = "grizzly-vs-rhino.map"
end_frame = 900

[[check]]
what = "count"
type = "MTNK"
owner = 0
at = 600

[[check]]
what = "event"
event = "removed"
type = "HTNK"
window = 30

[[check]]
what = "position"
type = "E1"
at = 300
tolerance = 64
"#,
        )
        .unwrap();
        assert_eq!(s.checks.len(), 3);
        assert_eq!(
            s.checks[0],
            Check::Count {
                filter: Filter {
                    type_id: Some("MTNK".into()),
                    owner: Some(0)
                },
                at: 600,
                tolerance: 0
            }
        );
        assert!(matches!(
            s.checks[1],
            Check::Event {
                event: EventKind::Removed,
                window: 30,
                ..
            }
        ));
        assert!(Scenario::parse("name='x'\nmap='m'\nend_frame=1\nbogus=1\n").is_err());
    }
}
