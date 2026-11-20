//! `ra2tool vxl`: voxel models and their animations.

use std::io;
use std::path::PathBuf;

use ra2_formats::ini::Ini;
use ra2_formats::vxl::{Hva, Vxl};
use ra2_vfs::install::Game;

use crate::pal::{read, vfs};

/// Parse every VXL and HVA in the mounted game, then pair them by the image
/// names the rules and art use and compare their section counts.
pub(crate) fn check(game: Game, game_dir: Option<PathBuf>) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let (mut vxls, mut hvas, mut voxels) = (0usize, 0usize, 0usize);
    let mut modes: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();
    let mut max_normal: std::collections::BTreeMap<u8, u8> = std::collections::BTreeMap::new();
    let mut failures = Vec::new();
    for m in vfs.mounts() {
        for e in &m.archive.index().entries {
            let Ok(data) = m.archive.read(*e) else {
                continue;
            };
            if data.starts_with(b"Voxel Animation") {
                match Vxl::parse(&data) {
                    Ok(v) => {
                        vxls += 1;
                        voxels += v.sections.iter().map(|s| s.voxels.len()).sum::<usize>();
                        for s in &v.sections {
                            *modes.entry(s.normals_mode).or_default() += 1;
                            let top = s.voxels.iter().map(|x| x.normal).max().unwrap_or(0);
                            let e = max_normal.entry(s.normals_mode).or_default();
                            *e = (*e).max(top);
                        }
                    }
                    Err(err) => failures.push(format!("{} {:08X}: {err}", m.name, e.id)),
                }
            } else if Hva::parse(&data).is_ok() {
                hvas += 1;
            }
        }
    }
    println!(
        "{vxls} VXLs ({voxels} voxels) and {hvas} HVAs parse; {} VXLs failed",
        failures.len()
    );
    for f in failures.iter().take(20) {
        println!("  {f}");
    }
    for (mode, n) in &modes {
        println!(
            "  normals mode {mode}: {n} sections, highest normal index {}",
            max_normal[mode]
        );
    }

    // Pair by name.
    let (rules, art) = if game == Game::YurisRevenge {
        ("rulesmd.ini", "artmd.ini")
    } else {
        ("rules.ini", "art.ini")
    };
    let rules = Ini::parse(&read(&vfs, rules)?);
    let art = Ini::parse(&read(&vfs, art)?);
    let mut images = std::collections::BTreeSet::new();
    for list in [
        "VehicleTypes",
        "AircraftTypes",
        "BuildingTypes",
        "VoxelAnims",
    ] {
        let Some(s) = rules.section(list) else {
            continue;
        };
        for (_, id) in s.entries() {
            let image = rules
                .get(id, "Image")
                .or_else(|| art.get(id, "Image"))
                .unwrap_or(id);
            images.insert(image.to_ascii_lowercase());
        }
    }
    let (mut pairs, mut mismatched) = (0, Vec::new());
    for image in &images {
        for part in ["", "tur", "barl"] {
            let base = format!("{image}{part}");
            let (Some(v), Some(h)) = (
                vfs.read(&format!("{base}.vxl"))?,
                vfs.read(&format!("{base}.hva"))?,
            ) else {
                continue;
            };
            let (Ok(v), Ok(h)) = (Vxl::parse(&v), Hva::parse(&h)) else {
                mismatched.push(format!("{base}: does not parse"));
                continue;
            };
            pairs += 1;
            if v.sections.len() != h.section_names.len() {
                mismatched.push(format!(
                    "{base}: {} sections in VXL, {} in HVA",
                    v.sections.len(),
                    h.section_names.len()
                ));
            }
        }
    }
    println!(
        "{} image names; {pairs} VXL+HVA pairs found by name; {} mismatched",
        images.len(),
        mismatched.len()
    );
    for m in mismatched.iter().take(20) {
        println!("  {m}");
    }
    Ok(failures.is_empty() && mismatched.is_empty())
}

/// Print a model's sections, and its animation if there is one.
pub(crate) fn info(game: Game, game_dir: Option<PathBuf>, name: &str) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let v = Vxl::parse(&read(&vfs, &format!("{name}.vxl"))?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    println!(
        "{name}.vxl: remap {}-{}, {} sections",
        v.remap.0,
        v.remap.1,
        v.sections.len()
    );
    for s in &v.sections {
        println!(
            "  {:<12} size {:?}, {} voxels, scale {}, normals mode {}, bounds {:?}..{:?}",
            s.name,
            s.size,
            s.voxels.len(),
            s.scale,
            s.normals_mode,
            s.min_bounds,
            s.max_bounds
        );
    }
    if let Some(h) = vfs.read(&format!("{name}.hva"))? {
        let h = Hva::parse(&h)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        println!(
            "{name}.hva: {} frames, sections {:?}",
            h.frames.len(),
            h.section_names
        );
    }
    Ok(true)
}
