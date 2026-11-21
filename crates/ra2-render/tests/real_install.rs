//! Checks against the developer's own install, when one is found. CI has no
//! game data, so there these tests report that they skipped and pass.

use ra2_render::voxel::{LIGHT, Normals};
use ra2_vfs::install::{Game, SearchContext};

#[test]
fn gamemd_normal_sets_read_and_light() {
    let Ok(install) = SearchContext::from_env().find(Game::YurisRevenge) else {
        eprintln!("skipped: no Yuri's Revenge install (set RA2_DATA)");
        return;
    };
    let Some(path) = install.inspection.file("gamemd.exe") else {
        return;
    };
    let exe = std::fs::read(path).unwrap();
    let n = Normals::from_gamemd(&exe).expect("gamemd.exe 1.001 normal sets");
    assert_eq!([1, 2, 3, 4].map(|m| n.count(m)), [16, 36, 64, 245]);
    let t = n.shade_table(4, LIGHT);
    assert!(
        t[..245].contains(&0) && t[..245].iter().any(|&l| l >= 16),
        "a spread of levels"
    );
    assert!(t[..245].iter().all(|&l| l <= 32));
}

#[test]
fn gamemd_shroud_frame_table_reads() {
    use ra2_render::shroud::{FrameTable, SOLID};
    let Ok(install) = SearchContext::from_env().find(Game::YurisRevenge) else {
        eprintln!("skipped: no Yuri's Revenge install (set RA2_DATA)");
        return;
    };
    let Some(path) = install.inspection.file("gamemd.exe") else {
        return;
    };
    let t = FrameTable::from_gamemd(&std::fs::read(path).unwrap()).expect("1.001 table");
    assert_eq!(t.frame(0), 0, "no unseen neighbours: nothing drawn");
    assert_eq!(t.frame(0xFF), SOLID, "all unseen: solid");
    assert_eq!(t.frame(0x80), 1, "only the (0, -1) neighbour unseen");
}
