//! `ra2tool vxl`: voxel models and their animations.

use std::io;
use std::path::{Path, PathBuf};

use ra2_formats::ini::Ini;
use ra2_formats::vxl::{Hva, Vxl};
use ra2_vfs::install::Game;

use crate::image::write_rgba;
use crate::pal::{palette, read, vfs};

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

/// Draw a model's sections from the side (x across, z up) and from above
/// (x across, y down), nearest voxel first, in a palette. A flat preview
/// for checking models; real projection and lighting are the renderer's.
pub(crate) fn png(
    game: Game,
    game_dir: Option<PathBuf>,
    name: &str,
    pal_name: &str,
    out: &Path,
) -> io::Result<bool> {
    const SCALE: usize = 3;
    let vfs = vfs(game, game_dir)?;
    let v = Vxl::parse(&read(&vfs, &format!("{name}.vxl"))?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let colors = palette(&vfs, pal_name)?.to_rgb8();
    let panels: Vec<_> = v
        .sections
        .iter()
        .map(|s| {
            (
                usize::from(s.size[0]),
                usize::from(s.size[1]),
                usize::from(s.size[2]),
            )
        })
        .collect();
    let w = panels.iter().map(|&(x, _, _)| x).sum::<usize>().max(1) * SCALE;
    let h = panels.iter().map(|&(_, y, z)| y + z).max().unwrap_or(1) * SCALE + SCALE;
    let mut rgba = vec![0u8; w * h * 4];
    let mut ox = 0;
    for (s, &(sx, sy, sz)) in v.sections.iter().zip(&panels) {
        // Side: keep the voxel nearest the viewer (highest y); top: highest z.
        let mut side = vec![None; sx * sz];
        let mut top = vec![None; sx * sy];
        for vox in &s.voxels {
            let (x, y, z) = (usize::from(vox.x), usize::from(vox.y), usize::from(vox.z));
            let si = (sz - 1 - z) * sx + x;
            if side[si].is_none_or(|(d, _)| y > d) {
                side[si] = Some((y, vox.color));
            }
            let ti = y * sx + x;
            if top[ti].is_none_or(|(d, _)| z > d) {
                top[ti] = Some((z, vox.color));
            }
        }
        let mut put = |px: usize, py: usize, c: u8| {
            let col = colors[usize::from(c)];
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    let p = ((py * SCALE + dy) * w + px * SCALE + dx) * 4;
                    rgba[p..p + 4].copy_from_slice(&[col[0], col[1], col[2], 255]);
                }
            }
        };
        for (i, cell) in side.iter().enumerate() {
            if let Some((_, c)) = cell {
                put(ox + i % sx, i / sx, *c);
            }
        }
        for (i, cell) in top.iter().enumerate() {
            if let Some((_, c)) = cell {
                put(ox + i % sx, sz + 1 + i / sx, *c);
            }
        }
        ox += sx;
    }
    write_rgba(
        out,
        u32::try_from(w).expect("small"),
        u32::try_from(h).expect("small"),
        &rgba,
    )?;
    println!(
        "{name}.vxl: {} sections; wrote {}",
        v.sections.len(),
        out.display()
    );
    Ok(true)
}
