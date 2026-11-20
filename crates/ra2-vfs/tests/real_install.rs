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
