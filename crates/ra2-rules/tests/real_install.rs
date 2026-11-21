//! Checks against the developer's own install, when one is found. CI has no
//! game data, so there these tests report that they skipped and pass.

use ra2_formats::ini::Ini;
use ra2_rules::{Armor, Kind, Rules};
use ra2_vfs::install::{Game, SearchContext};
use ra2_vfs::mount::Vfs;

#[test]
fn yuris_revenge_rules_build() {
    let Ok(install) = SearchContext::from_env().find(Game::YurisRevenge) else {
        eprintln!("skipped: no Yuri's Revenge install (set RA2_DATA)");
        return;
    };
    let mut vfs = Vfs::new(install.inspection.clone());
    vfs.mount_startup(Game::YurisRevenge);
    let read = |n: &str| Ini::parse(&vfs.read(n).unwrap().unwrap());
    let rules = Rules::from_ini(&read("rulesmd.ini"), &read("artmd.ini"));
    let count = |k| rules.technos.iter().filter(|t| t.kind == k).count();
    assert!(count(Kind::Building) > 300, "{}", count(Kind::Building));
    assert!(count(Kind::Vehicle) > 70);
    assert!(count(Kind::Infantry) > 50);
    assert!(count(Kind::Aircraft) > 10);
    let grizzly = rules.techno("MTNK").unwrap();
    assert_eq!(
        (grizzly.strength, grizzly.armor, grizzly.cost),
        (300, Armor::Heavy, 700)
    );
    let gun = rules.weapon(grizzly.primary.unwrap());
    assert_eq!(
        (gun.id.as_str(), gun.damage, gun.rof, gun.range),
        ("105mm", 65, 60, 1280)
    );
    let ap = rules.warhead(gun.warhead.unwrap());
    assert_eq!(ap.verses[Armor::Plate.index()].of(gun.damage), 9);
    assert_eq!(rules.techno("GAPOWR").unwrap().foundation, (2, 2));
    assert!(
        rules
            .countries
            .iter()
            .any(|c| c.id == "Americans" && c.side == "GDI")
    );
    assert_eq!(rules.sides.len(), 5);
    assert_eq!(rules.super_weapons.len(), 12);
    let nuke = rules
        .super_weapons
        .iter()
        .find(|s| s.id == "NukeSpecial")
        .unwrap();
    assert_eq!(nuke.kind, "MultiMissile");
    assert_eq!(rules.weapon(nuke.weapon.unwrap()).id, "NukeCarrier");
    assert!(rules.anims.len() > 500);
    assert!(
        rules
            .particle_systems
            .iter()
            .any(|p| p.id == "SparkSys" && p.behaves_like == "Spark")
    );
    // Every weapon a type names resolves, and so does its warhead.
    assert!(
        rules
            .weapons
            .iter()
            .all(|w| w.warhead.is_some() || w.damage == 0)
    );
}
