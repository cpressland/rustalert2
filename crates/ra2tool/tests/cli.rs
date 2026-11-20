//! Run `ra2tool` against a synthetic install: archives, an INI, a string
//! table, a palette and a sprite, all made by these tests' own encoders. No
//! game data is involved, so these run in CI.

use std::path::Path;
use std::process::Command;

use ra2_formats::mix::mix_id;

/// A plain extended-layout MIX of (name, bytes).
fn mix(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut body = Vec::new();
    let mut entries = Vec::new();
    for (name, data) in files {
        entries.push((
            mix_id(name),
            u32::try_from(body.len()).unwrap(),
            u32::try_from(data.len()).unwrap(),
        ));
        body.extend_from_slice(data);
    }
    let mut out = 0u32.to_le_bytes().to_vec();
    out.extend_from_slice(&u16::try_from(files.len()).unwrap().to_le_bytes());
    out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
    for (id, off, size) in entries {
        for v in [id, off, size] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(&body);
    out
}

/// A one-label string table.
fn csf(label: &str, text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [u32::from_le_bytes(*b" FSC"), 3, 1, 1, 0, 0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(b" LBL");
    out.extend_from_slice(&1u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(label.len()).unwrap().to_le_bytes());
    out.extend_from_slice(label.as_bytes());
    out.extend_from_slice(b" RTS");
    let units: Vec<u16> = text.encode_utf16().collect();
    out.extend_from_slice(&u32::try_from(units.len()).unwrap().to_le_bytes());
    for u in units {
        out.extend_from_slice(&(!u).to_le_bytes());
    }
    out
}

/// A 4×2 one-frame raw SHP.
fn shp() -> Vec<u8> {
    let mut out = Vec::new();
    for v in [0u16, 4, 2, 1] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for v in [0u16, 0, 4, 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&[0; 12]);
    out.extend_from_slice(&32u32.to_le_bytes());
    out.extend_from_slice(&[1, 2, 3, 16, 0, 17, 18, 0]);
    out
}

fn install(dir: &Path) {
    let rules = b"[General]\nName=Synthetic\n[Colors]\nDarkRed=0,230,255\n";
    let palette: Vec<u8> = (0..=255u8).flat_map(|i| [i >> 2, i >> 2, i >> 2]).collect();
    let local = mix(&[
        ("rulesmd.ini", rules),
        ("unittem.pal", &palette),
        ("test.shp", &shp()),
    ]);
    let strings = csf("Name:Test", "Test  Unit ");
    std::fs::write(dir.join("ra2.mix"), mix(&[])).unwrap();
    std::fs::write(dir.join("language.mix"), mix(&[])).unwrap();
    std::fs::write(dir.join("ra2md.mix"), mix(&[("localmd.mix", &local)])).unwrap();
    std::fs::write(dir.join("langmd.mix"), mix(&[("ra2md.csf", &strings)])).unwrap();
}

fn run(dir: &Path, args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ra2tool"))
        .args(args)
        .env("RA2_DATA", dir)
        .env("HOME", dir)
        .output()
        .unwrap();
    let text =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "ra2tool {args:?} failed:\n{text}");
    text
}

#[test]
fn tools_work_on_a_synthetic_install() {
    let t = tempfile::tempdir().unwrap();
    let dir = t.path();
    install(dir);

    let doctor = run(dir, &["doctor"]);
    assert!(
        doctor
            .lines()
            .any(|l| l.starts_with("Yuri's Revenge:") && l.ends_with(" ok")),
        "{doctor}"
    );

    let ls = run(dir, &["mix", "ls", "ra2md.mix/localmd.mix"]);
    assert!(ls.contains("3 files") && ls.contains("rulesmd.ini"), "{ls}");

    let x = run(dir, &["mix", "x", "ra2md.mix/localmd.mix", "rulesmd.ini"]);
    assert!(x.starts_with("[General]"), "{x}");

    let which = run(dir, &["vfs", "which", "rulesmd.ini"]);
    assert!(which.contains("localmd.mix"), "{which}");

    assert_eq!(
        run(dir, &["ini", "get", "rulesmd.ini", "General", "Name"]).trim(),
        "Synthetic"
    );

    let text = run(dir, &["csf", "get", "name:test"]);
    assert!(
        text.contains("Test Unit") && !text.contains("Test  Unit"),
        "the clean-up applies: {text}"
    );

    let ramp = run(dir, &["pal", "ramp", "DarkRed"]);
    assert!(
        ramp.starts_with("DarkRed") && ramp.contains("000000"),
        "{ramp}"
    );

    let png = dir.join("test.png");
    let shp = run(
        dir,
        &[
            "shp",
            "png",
            "test.shp",
            "--house",
            "DarkRed",
            "-o",
            png.to_str().unwrap(),
        ],
    );
    assert!(shp.contains("4x2, 1 frames"), "{shp}");
    assert!(std::fs::metadata(&png).unwrap().len() > 0);

    let check = run(dir, &["shp", "check"]);
    assert!(
        check.contains("1 parse as SHP") && check.contains("0 files parsed but failed"),
        "{check}"
    );
}

#[test]
fn doctor_reports_what_is_missing() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("ra2.mix"), mix(&[])).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_ra2tool"))
        .arg("doctor")
        .env("RA2_DATA", t.path())
        .env("HOME", t.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("missing") && err.contains("ra2md.mix"),
        "{err}"
    );
}
