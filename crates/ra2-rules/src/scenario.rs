//! Reading a map's starting objects into plain placements the simulation
//! builds its world from. See `docs/spec/objects-model.md`.

use ra2_formats::ini::{Ini, atoi};

use crate::Kind;

/// One object a map places at the start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub kind: Kind,
    /// The rules ID.
    pub type_id: String,
    /// The owning house, by name (`Americans`, `Neutral`…).
    pub owner: String,
    /// The cell: for buildings the foundation's top cell.
    pub cell: (i16, i16),
    /// Infantry's spot in the cell, 0 (centre) to 4.
    pub sub_cell: u8,
    /// Health out of 256.
    pub health: i32,
    /// Facing, 0–255.
    pub facing: u8,
    /// The mission it starts on (`Guard`, `Hunt`, `Sleep`…).
    pub mission: String,
}

fn cell(x: &str, y: &str) -> (i16, i16) {
    (
        i16::try_from(atoi(x)).unwrap_or(0),
        i16::try_from(atoi(y)).unwrap_or(0),
    )
}

fn byte(v: &str) -> u8 {
    u8::try_from(atoi(v).rem_euclid(256)).expect("0..256")
}

/// Every starting object in a map's `[Infantry]`, `[Units]`, `[Aircraft]`
/// and `[Structures]`, in that order and in each section's order.
/// `[hypothesis]` (the order the game creates them in)
///
/// Line formats (`owner,type,health,x,y,…`): `[community]`
/// - infantry: `…,x,y,sub-cell,mission,facing,…`
/// - units: `…,x,y,facing,mission,…`
/// - aircraft: `…,x,y,facing,mission,…`
/// - structures: `…,x,y,facing,…` (no mission)
pub fn placements(map: &Ini) -> Vec<Placement> {
    let mut out = Vec::new();
    for (section, kind) in [
        ("Infantry", Kind::Infantry),
        ("Units", Kind::Vehicle),
        ("Aircraft", Kind::Aircraft),
        ("Structures", Kind::Building),
    ] {
        let Some(s) = map.section(section) else {
            continue;
        };
        for (_, v) in s.entries() {
            let p: Vec<&str> = v.split(',').collect();
            let need = if kind == Kind::Infantry { 8 } else { 6 };
            if p.len() < need {
                continue;
            }
            let (sub_cell, facing, mission) = match kind {
                Kind::Infantry => (byte(p[5]), byte(p[7]), p[6]),
                Kind::Building => (0, byte(p[5]), "Guard"),
                _ => (0, byte(p[5]), p[6]),
            };
            out.push(Placement {
                kind,
                type_id: p[1].trim().to_string(),
                owner: p[0].trim().to_string(),
                cell: cell(p[3], p[4]),
                sub_cell: sub_cell.min(4),
                health: atoi(p[2]),
                facing,
                mission: mission.trim().to_string(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_kinds_line_format() {
        let map = Ini::parse(
            b"[Units]\n0=Neutral,MTNK,256,47,50,64,Hunt,None,0,-1,0,-1,1,1\n\
              [Infantry]\n0=Neutral,E1,128,47,50,2,Guard,96,-1,0,0,1,1,0\n\
              [Structures]\n0=Americans,GAPOWR,256,10,12,0,None,1,0,1,0,0,None,None,None,0,0\n",
        );
        let p = placements(&map);
        assert_eq!(p.len(), 3);
        assert_eq!(
            (p[0].kind, p[0].sub_cell, p[0].facing, p[0].mission.as_str()),
            (Kind::Infantry, 2, 96, "Guard")
        );
        assert_eq!(
            (p[1].type_id.as_str(), p[1].cell, p[1].facing),
            ("MTNK", (47, 50), 64)
        );
        assert_eq!(
            (p[2].kind, p[2].owner.as_str(), p[2].cell),
            (Kind::Building, "Americans", (10, 12))
        );
    }
}
