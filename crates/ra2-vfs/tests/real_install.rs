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

#[test]
fn yr_string_table_reads() {
    use ra2_formats::csf::{Language, StringTable};
    use ra2_vfs::install::Inspection;
    use ra2_vfs::mount::Vfs;
    let Some(install) = install() else { return };
    let mut vfs = Vfs::new(Inspection::of(&install.inspection.root).unwrap());
    vfs.mount_startup(Game::YurisRevenge);
    let t = StringTable::parse(&vfs.read("ra2md.csf").unwrap().unwrap()).unwrap();
    assert!(t.entries().len() > 5000);
    assert_eq!(t.duplicates, Vec::<String>::new());
    if t.language == Language::Us {
        assert_eq!(t.text("name:e1"), "GI");
        assert_eq!(t.text("NAME:GAPOWR"), "Power Plant");
    }
    assert!(
        t.entries()
            .iter()
            .all(|e| !e.text.contains("  ") && !e.text.ends_with(' '))
    );
}

#[test]
fn yr_palettes_and_house_colours() {
    use ra2_formats::ini::Ini;
    use ra2_formats::pal::{Hsv, Palette, house_ramp};
    use ra2_vfs::install::Inspection;
    use ra2_vfs::mount::Vfs;
    let Some(install) = install() else { return };
    let mut vfs = Vfs::new(Inspection::of(&install.inspection.root).unwrap());
    vfs.mount_startup(Game::YurisRevenge);
    for name in ["unittem.pal", "temperat.pal", "anim.pal", "palette.pal"] {
        let pal =
            Palette::parse(&vfs.read(name).unwrap().unwrap()).unwrap_or_else(|| panic!("{name}"));
        assert!(pal.colors.iter().flatten().all(|&v| v <= 63));
    }
    let rules = Ini::parse(&vfs.read("rulesmd.ini").unwrap().unwrap());
    let colours = rules.section("Colors").unwrap();
    assert!(colours.len() >= 20);
    for (_, value) in colours.entries() {
        let _ = house_ramp(Hsv::parse(value));
    }
}

#[test]
fn yr_shps_decode() {
    use ra2_formats::shp::Shp;
    use ra2_vfs::install::Inspection;
    use ra2_vfs::mount::Vfs;
    let Some(install) = install() else { return };
    let mut vfs = Vfs::new(Inspection::of(&install.inspection.root).unwrap());
    vfs.mount_startup(Game::YurisRevenge);
    let (mut files, mut frames) = (0, 0);
    for m in vfs.mounts() {
        for e in &m.archive.index().entries {
            let data = m.archive.read(*e).unwrap();
            let Ok(shp) = Shp::parse(&data) else { continue };
            files += 1;
            for k in 0..shp.frames.len() {
                shp.frame(k)
                    .unwrap_or_else(|err| panic!("{} {:08X}: {err}", m.name, e.id));
                frames += 1;
            }
        }
    }
    assert!(
        files > 1000 && frames > 50_000,
        "{files} files, {frames} frames"
    );
    let gi = vfs.read("gi.shp").unwrap().unwrap();
    assert!(Shp::parse(&gi).unwrap().frames.len() > 100);
}

#[test]
fn yr_voxels_parse() {
    use ra2_formats::vxl::{Hva, Vxl};
    use ra2_vfs::install::Inspection;
    use ra2_vfs::mount::Vfs;
    let Some(install) = install() else { return };
    let mut vfs = Vfs::new(Inspection::of(&install.inspection.root).unwrap());
    vfs.mount_startup(Game::YurisRevenge);
    let mut models = 0;
    for m in vfs.mounts() {
        for e in &m.archive.index().entries {
            let data = m.archive.read(*e).unwrap();
            if data.starts_with(b"Voxel Animation") {
                Vxl::parse(&data).unwrap_or_else(|err| panic!("{} {:08X}: {err}", m.name, e.id));
                models += 1;
            }
        }
    }
    assert!(models > 200, "{models}");
    let hull = Vxl::parse(&vfs.read("htnk.vxl").unwrap().unwrap()).unwrap();
    let anim = Hva::parse(&vfs.read("htnk.hva").unwrap().unwrap()).unwrap();
    assert_eq!(hull.sections.len(), anim.section_names.len());
}

#[test]
fn yr_theater_tiles_parse() {
    use ra2_formats::ini::Ini;
    use ra2_formats::tmp::{Tmp, tile_sets};
    use ra2_vfs::install::Inspection;
    use ra2_vfs::mount::Vfs;
    use ra2_vfs::theater::Theater;
    let Some(install) = install() else { return };
    for theater in Theater::ALL {
        let mut vfs = Vfs::new(Inspection::of(&install.inspection.root).unwrap());
        vfs.mount_startup(Game::YurisRevenge);
        vfs.mount_theater(theater);
        let ini = Ini::parse(&vfs.read(&theater.ini()).unwrap().unwrap());
        let mut parsed = 0;
        for set in tile_sets(&ini) {
            for n in 1..=set.tiles {
                let file = format!("{}{n:02}.{}", set.file_name, theater.ext());
                if let Some(data) = vfs.read(&file).unwrap() {
                    Tmp::parse(&data).unwrap_or_else(|e| panic!("{file}: {e}"));
                    parsed += 1;
                }
            }
        }
        assert!(parsed > 100, "{theater:?}: {parsed}");
    }
}
