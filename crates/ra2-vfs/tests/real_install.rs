//! Checks against the developer's own install, when one is found. CI has no
//! game data, so there these tests report that they skipped and pass.

use ra2_vfs::archive::Archive;
use ra2_vfs::install::{Game, Install, SearchContext};

fn install() -> Option<Install> {
    let found = SearchContext::from_env().find(Game::YurisRevenge).ok();
    if found.is_none() {
        eprintln!("skipped: no Yuri's Revenge install (set RA2_DATA)");
    }
    found
}

#[test]
fn yr_archives_open_and_verify() {
    let Some(install) = install() else { return };
    let ra2md = Archive::open(&install.inspection.file("ra2md.mix").unwrap()).unwrap();
    assert!(ra2md.index().is_encrypted());
    assert_eq!(ra2md.verify_checksum().unwrap(), Some(true));
    let local = ra2md
        .open_nested(ra2md.find("localmd.mix").expect("localmd.mix in ra2md.mix"))
        .unwrap();
    let rules = local
        .read_file("rulesmd.ini")
        .unwrap()
        .expect("rulesmd.ini in localmd.mix");
    assert!(
        rules.starts_with(b"; RULE"),
        "rulesmd.ini should be the INI text"
    );
}

#[test]
fn yr_startup_mounts_and_resolves() {
    use ra2_vfs::install::Inspection;
    use ra2_vfs::mount::{Location, Mounted, Vfs};
    let Some(install) = install() else { return };
    let mut vfs = Vfs::new(Inspection::of(&install.inspection.root).unwrap());
    let results = vfs.mount_startup(Game::YurisRevenge);
    assert!(
        !results.iter().any(|r| matches!(r, Mounted::Failed { .. })),
        "{results:?}"
    );
    for name in ["ra2md.mix", "localmd.mix", "langmd.mix", "conqmd.mix"] {
        assert!(
            vfs.mounts().iter().any(|m| m.name == name),
            "{name} not mounted"
        );
    }
    for file in [
        "rulesmd.ini",
        "artmd.ini",
        "aimd.ini",
        "ra2md.csf",
        "mouse.sha",
    ] {
        assert!(
            matches!(
                vfs.locate(file),
                Some(Location::Archive { .. } | Location::Loose(_))
            ),
            "{file}"
        );
    }
    let rules = vfs.read("rulesmd.ini").unwrap().unwrap();
    assert!(rules.starts_with(b";"), "rulesmd.ini should be INI text");
}

#[test]
fn yr_rules_inis_parse() {
    use ra2_formats::ini::Ini;
    use ra2_vfs::install::Inspection;
    use ra2_vfs::mount::Vfs;
    let Some(install) = install() else { return };
    let mut vfs = Vfs::new(Inspection::of(&install.inspection.root).unwrap());
    vfs.mount_startup(Game::YurisRevenge);
    let rules = Ini::parse(&vfs.read("rulesmd.ini").unwrap().unwrap());
    assert!(rules.sections().len() > 1000);
    assert_eq!(rules.get_list("InfantryTypes", "1").unwrap(), ["E1"]);
    assert_eq!(rules.duplicates, []);
    // The empty first [PARABOMB] must not shadow the real one.
    let art = Ini::parse(&vfs.read("artmd.ini").unwrap().unwrap());
    assert_eq!(art.get_int("PARABOMB", "Rate"), Some(200));
    assert_eq!(art.duplicates, []);
}
