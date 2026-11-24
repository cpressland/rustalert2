//! Writing scenario maps: a flat clear map with pre-placed objects owned by
//! country houses, so a scenario plays out with no input. The houses are
//! hostile to each other unless their `allies` say otherwise; objects given
//! `Hunt` seek out enemies by themselves.
//!
//! The map is a Yuri's Revenge multiplayer map (`.yrm`). YR lists loose
//! `*.YRM` files in its skirmish menu (`[recomp 0x0083F490]`).

use std::fmt::Write as _;

use ra2_formats::codec::{Compression, base64_encode, pack_blocks_stored};
use serde::Deserialize;

/// A generated map, from the scenario's `[generate]` table.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapSpec {
    /// Shown in the skirmish menu.
    pub name: String,
    #[serde(default = "default_theater")]
    pub theater: String,
    pub width: i32,
    pub height: i32,
    /// Skirmish start points (cells), at least two; far from the action.
    pub starts: Vec<[i32; 2]>,
    #[serde(default, rename = "house")]
    pub houses: Vec<HouseSpec>,
    #[serde(default, rename = "unit")]
    pub units: Vec<ObjectSpec>,
    #[serde(default, rename = "infantry")]
    pub infantry: Vec<ObjectSpec>,
    #[serde(default, rename = "structure")]
    pub structures: Vec<ObjectSpec>,
    /// Ore and gem fields.
    #[serde(default)]
    pub ore: Vec<OreSpec>,
    /// Let ore grow and spread (`TiberiumGrows=`, `TiberiumSpreads=`); off
    /// by default, so a scenario's ore stays as laid.
    #[serde(default)]
    pub ore_grows: bool,
    /// INI text added to the map as written: rules sections the game (and
    /// our engine) loads over `rulesmd.ini`, e.g. shorter recharge times so
    /// a scenario records quickly.
    #[serde(default)]
    pub rules: Option<String>,
}

/// A rectangle of ore (or gems) on the map.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OreSpec {
    /// The top cell and size, in map cells.
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    /// The overlay's position in the rules' `[OverlayTypes]` (102 is
    /// `TIB01`, ore).
    #[serde(default = "default_ore")]
    pub overlay: u8,
    /// The growth stage, 0 to 11 (11 is full).
    #[serde(default = "full_ore")]
    pub density: u8,
}

fn default_ore() -> u8 {
    102
}

fn full_ore() -> u8 {
    11
}

fn default_theater() -> String {
    "TEMPERATE".into()
}

/// A country house that owns pre-placed objects.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HouseSpec {
    /// A country from the rules' `[Countries]`, e.g. `Americans`.
    pub name: String,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub allies: Vec<String>,
    #[serde(default)]
    pub credits: i32,
}

/// One pre-placed object.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectSpec {
    pub owner: String,
    #[serde(rename = "type")]
    pub type_id: String,
    /// Cell coordinates.
    pub x: i32,
    pub y: i32,
    /// 0 to 255; 0 is north.
    #[serde(default)]
    pub facing: u8,
    /// `Guard`, `Hunt`, `Harvest`, `Sleep`, …
    #[serde(default = "default_mission")]
    pub mission: String,
    /// Health out of 256.
    #[serde(default = "full_health")]
    pub health: u16,
    /// Infantry sub-cell, 0 to 4.
    #[serde(default)]
    pub subcell: u8,
    /// Units and infantry: veterancy, 100 a veteran and 200 elite.
    #[serde(default)]
    pub veterancy: u16,
}

fn default_mission() -> String {
    "Guard".into()
}

fn full_health() -> u16 {
    256
}

/// The cells inside a `width × height` map, in the iso coordinates map
/// packs use: `0 <= x - y + w - 1 < 2w - 1` and `0 <= x + y - w - 1 < 2h`.
/// This gives exactly the cells of the campaign maps' packs (15,480 for a
/// 65 × 120 map); the first version allowed `dx = 2w - 1`, `h` cells past
/// the game's cell array, and the game faulted loading it.
pub fn cells(width: i32, height: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::new();
    for y in 0..=width + height {
        for x in 0..=width + height + width {
            let (dx, dy) = (x - y + width - 1, x + y - width - 1);
            if (0..2 * width - 1).contains(&dx) && (0..2 * height).contains(&dy) {
                out.push((x, y));
            }
        }
    }
    out
}

fn packed_section(ini: &mut String, name: &str, data: &[u8], compression: Compression) {
    let text = base64_encode(&pack_blocks_stored(data, compression));
    writeln!(ini, "[{name}]").unwrap();
    for (i, line) in text.as_bytes().chunks(70).enumerate() {
        writeln!(
            ini,
            "{}={}",
            i + 1,
            std::str::from_utf8(line).expect("base64 is ASCII")
        )
        .unwrap();
    }
    ini.push('\n');
}

/// Terrain (every cell clear: tile 0, level 0) and the overlays: none,
/// except the ore fields.
fn write_terrain(ini: &mut String, w: i32, h: i32, ore: &[OreSpec]) {
    let mut iso = Vec::new();
    for (x, y) in cells(w, h) {
        iso.extend_from_slice(&i16::try_from(x).expect("small map").to_le_bytes());
        iso.extend_from_slice(&i16::try_from(y).expect("small map").to_le_bytes());
        iso.extend_from_slice(&0i32.to_le_bytes());
        iso.extend_from_slice(&[0, 0, 0]);
    }
    iso.extend_from_slice(&[0; 4]);
    packed_section(ini, "IsoMapPack5", &iso, Compression::Lzo);
    let (mut overlay, mut data) = (vec![0xffu8; 512 * 512], vec![0u8; 512 * 512]);
    for o in ore {
        for y in o.y..o.y + o.height {
            for x in o.x..o.x + o.width {
                if let Ok(i) = usize::try_from(y * 512 + x)
                    && i < overlay.len()
                {
                    overlay[i] = o.overlay;
                    data[i] = o.density;
                }
            }
        }
    }
    packed_section(ini, "OverlayPack", &overlay, Compression::Lcw);
    packed_section(ini, "OverlayDataPack", &data, Compression::Lcw);
}

fn yes_no(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

/// Write the map as INI text.
pub fn write(spec: &MapSpec) -> String {
    let mut ini = String::new();
    let (w, h) = (spec.width, spec.height);
    let _ = write!(
        ini,
        "; Written by rustalert2's parity lab. No game content.\n\n\
         [Basic]\nName={}\nPercent=0\nGameMode=standard\nMultiplayerOnly=1\nMinPlayer=2\nMaxPlayer={}\n\
         Official=no\nEndOfGame=no\nSkipScore=yes\nOneTimeOnly=no\nSkipMapSelect=no\nTruckCrate=no\nTrainCrate=no\n\
         NewINIFormat=4\nInitTime=10000\nFreeRadar=no\nTiberiumGrowthEnabled={}\nVeinGrowthEnabled=no\n\
         IceGrowthEnabled=no\nTiberiumDeathToVisceroid=no\n\n",
        spec.name,
        spec.starts.len(),
        yes_no(spec.ore_grows)
    );
    let _ = write!(
        ini,
        "[Map]\nTheater={}\nSize=0,0,{w},{h}\nLocalSize=2,4,{},{}\n\n",
        spec.theater,
        w - 4,
        h - 8
    );
    let _ = write!(
        ini,
        "[SpecialFlags]\nTiberiumGrows={g}\nTiberiumSpreads={g}\nTiberiumExplosive=no\nDestroyableBridges=yes\n\
         MCVDeploy=no\nInitialVeteran=no\nFixedAlliance=no\nHarvesterImmune=no\nFogOfWar=no\nInert=no\n\
         IonStorms=no\nMeteorites=no\nVisceroids=no\n\n\
         [Lighting]\nAmbient=1.000000\nRed=1.000000\nGreen=1.000000\nBlue=1.000000\nGround=0.000000\nLevel=0.000000\n\
         IonAmbient=1.000000\nIonRed=1.000000\nIonGreen=1.000000\nIonBlue=1.000000\nIonGround=0.000000\nIonLevel=0.000000\n\n",
        g = yes_no(spec.ore_grows)
    );

    ini.push_str("[Waypoints]\n");
    for (i, [x, y]) in spec.starts.iter().enumerate() {
        let _ = writeln!(ini, "{i}={}", y * 1000 + x);
    }
    ini.push('\n');

    if !spec.houses.is_empty() {
        ini.push_str("[Houses]\n");
        for (i, hs) in spec.houses.iter().enumerate() {
            let _ = writeln!(ini, "{i}={}", hs.name);
        }
        ini.push('\n');
        for hs in &spec.houses {
            let mut allies = vec![hs.name.clone()];
            allies.extend(hs.allies.iter().cloned());
            let _ = write!(
                ini,
                "[{0}]\nCountry={0}\nTechLevel=10\nCredits={1}\nIQ=0\nEdge=North\nPlayerControl=no\nColor={2}\n\
                 Allies={3}\nPercentBuilt=100\nNodeCount=0\n\n",
                hs.name,
                hs.credits,
                hs.color.as_deref().unwrap_or("Grey"),
                allies.join(",")
            );
        }
    }

    let objects = |ini: &mut String,
                   section: &str,
                   list: &[ObjectSpec],
                   line: &dyn Fn(&ObjectSpec) -> String| {
        if list.is_empty() {
            return;
        }
        let _ = writeln!(ini, "[{section}]");
        for (i, o) in list.iter().enumerate() {
            let _ = writeln!(ini, "{i}={}", line(o));
        }
        ini.push('\n');
    };
    objects(&mut ini, "Units", &spec.units, &|o| {
        format!(
            "{},{},{},{},{},{},{},None,{},-1,0,-1,0,0",
            o.owner, o.type_id, o.health, o.x, o.y, o.facing, o.mission, o.veterancy
        )
    });
    objects(&mut ini, "Infantry", &spec.infantry, &|o| {
        format!(
            "{},{},{},{},{},{},{},{},None,{},-1,0,0,0",
            o.owner, o.type_id, o.health, o.x, o.y, o.subcell, o.mission, o.facing, o.veterancy
        )
    });
    objects(&mut ini, "Structures", &spec.structures, &|o| {
        format!(
            "{},{},{},{},{},{},None,1,0,1,0,0,None,None,None,0,0",
            o.owner, o.type_id, o.health, o.x, o.y, o.facing
        )
    });

    write_terrain(&mut ini, w, h, &spec.ore);
    if let Some(rules) = &spec.rules {
        ini.push('\n');
        ini.push_str(rules.trim());
        ini.push('\n');
    }
    ini
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2_formats::ini::Ini;
    use ra2_formats::map::{Map, OVERLAY_SIZE};

    #[test]
    fn written_map_reads_back() {
        let spec: MapSpec = toml::from_str(
            r#"
name = "Parity: test"
width = 30
height = 30
starts = [[10, 40], [40, 10]]
[[house]]
name = "Americans"
color = "DarkBlue"
[[house]]
name = "Russians"
[[unit]]
owner = "Americans"
type = "MTNK"
x = 25
y = 25
mission = "Hunt"
[[infantry]]
owner = "Russians"
type = "E2"
x = 30
y = 30
subcell = 2
"#,
        )
        .unwrap();
        let text = write(&spec);
        let ini = Ini::parse(text.as_bytes());
        let map = Map::from_ini(&ini).unwrap();
        assert_eq!((map.size.width, map.size.height), (30, 30));
        assert_eq!(map.tiles.len(), cells(30, 30).len());
        assert_eq!(map.iso_trailing, [0, 0, 0, 0]);
        assert_eq!(map.overlay.unwrap().len(), OVERLAY_SIZE);
        assert_eq!(
            ini.get("Units", "0"),
            Some("Americans,MTNK,256,25,25,0,Hunt,None,0,-1,0,-1,0,0")
        );
        assert_eq!(ini.get("Russians", "Allies"), Some("Russians"));
        assert_eq!(ini.get("Waypoints", "1"), Some("10040"));
    }

    #[test]
    fn cell_rule_matches_shipped_counts() {
        // all01umd.map (65 x 120) and sov01umd.map (58 x 140) list exactly
        // these many cells, all inside the rule.
        assert_eq!(cells(65, 120).len(), 15_480);
        assert_eq!(cells(58, 140).len(), 16_100);
    }
}
