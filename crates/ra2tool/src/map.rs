//! `ra2tool map`: decode scenario files.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use ra2_formats::ini::Ini;
use ra2_formats::map::Map;
use ra2_formats::tmp::{TileSet, tile_sets};
use ra2_vfs::archive::Archive;
use ra2_vfs::install::Game;
use ra2_vfs::theater::Theater;

use crate::image::write_rgba;
use crate::pal::{read, vfs};

fn looks_like_map(data: &[u8]) -> bool {
    data.windows(5).take(200_000).any(|w| w == b"[Map]")
}

/// Every map in the mounted game and in loose map archives (`.mmx`, `.yro`)
/// and map files in the game folder: (where, bytes).
fn all_maps(game: Game, game_dir: Option<&Path>) -> io::Result<Vec<(String, Vec<u8>)>> {
    let vfs = vfs(game, game_dir.map(Path::to_path_buf))?;
    let mut out = Vec::new();
    let scan = |out: &mut Vec<(String, Vec<u8>)>, label: &str, archive: &Archive| {
        for e in &archive.index().entries {
            if let Ok(data) = archive.read(*e)
                && looks_like_map(&data)
            {
                out.push((format!("{label} {:08X}", e.id), data));
            }
        }
    };
    for m in vfs.mounts() {
        scan(&mut out, &m.name, &m.archive);
    }
    let root = vfs.locate("ra2md.mix").and_then(|l| match l {
        ra2_vfs::mount::Location::Loose(p) => p.parent().map(Path::to_path_buf),
        ra2_vfs::mount::Location::Archive { .. } => None,
    });
    if let Some(root) = root {
        let mut names: Vec<_> = std::fs::read_dir(&root)?
            .flatten()
            .map(|e| e.path())
            .collect();
        names.sort();
        for p in names {
            let ext = p
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let label = p
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if matches!(ext.as_str(), "mmx" | "yro") {
                if let Ok(a) = Archive::open(&p) {
                    scan(&mut out, &label, &a);
                }
            } else if matches!(ext.as_str(), "map" | "yrm" | "mpr") {
                out.push((label, std::fs::read(&p)?));
            }
        }
    }
    Ok(out)
}

/// Decode every map's packs, and check its tile numbers against its theater.
pub(crate) fn check(game: Game, game_dir: Option<&Path>) -> io::Result<bool> {
    // Per theater: each tile number's set, and whether its file exists.
    struct TheaterTiles {
        sets: Vec<TileSet>,
        set_of: Vec<usize>,
        exists: Vec<bool>,
    }
    let mut theaters: BTreeMap<Theater, TheaterTiles> = BTreeMap::new();
    for t in Theater::ALL {
        let mut v = vfs(game, game_dir.map(Path::to_path_buf))?;
        v.mount_theater(t);
        let Some(ini) = v.read(&t.ini())? else {
            continue;
        };
        let sets = tile_sets(&Ini::parse(&ini));
        let (mut set_of, mut exists) = (Vec::new(), Vec::new());
        for (i, s) in sets.iter().enumerate() {
            for n in 1..=s.tiles {
                set_of.push(i);
                exists.push(
                    v.locate(&format!("{}{n:02}.{}", s.file_name, t.ext()))
                        .is_some(),
                );
            }
        }
        theaters.insert(
            t,
            TheaterTiles {
                sets,
                set_of,
                exists,
            },
        );
    }
    let maps = all_maps(game, game_dir)?;
    let (mut ok, mut tiles, mut previews, mut overlays) = (0, 0usize, 0, 0);
    let (mut out_of_range, mut marble, mut missing) = (0usize, 0usize, 0usize);
    let mut missing_sets: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_theater: BTreeMap<String, usize> = BTreeMap::new();
    let mut trailing: BTreeMap<Vec<u8>, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for (label, data) in &maps {
        match Map::from_ini(&Ini::parse(data)) {
            Ok(m) => {
                ok += 1;
                *trailing.entry(m.iso_trailing.clone()).or_default() += 1;
                tiles += m.tiles.len();
                previews += usize::from(m.preview.is_some());
                overlays += usize::from(m.overlay.is_some());
                *by_theater.entry(m.theater.to_uppercase()).or_default() += 1;
                let Some(tt) = Theater::from_name(&m.theater).and_then(|t| theaters.get(&t)) else {
                    continue;
                };
                for t in &m.tiles {
                    let Ok(n) = usize::try_from(t.tile) else {
                        continue;
                    };
                    if n == 0xFFFF {
                        continue;
                    }
                    if n >= tt.exists.len() {
                        out_of_range += 1;
                        *missing_sets
                            .entry(format!(
                                "{} past the last set (tile {n} of {})",
                                m.theater,
                                tt.exists.len()
                            ))
                            .or_default() += 1;
                    } else if !tt.exists[n] {
                        let set = &tt.sets[tt.set_of[n]];
                        if set.is_marble_madness() {
                            marble += 1;
                        } else {
                            missing += 1;
                            *missing_sets
                                .entry(format!("{} {}", m.theater, set.name))
                                .or_default() += 1;
                        }
                    }
                }
            }
            Err(e) => failures.push(format!("{label}: {e}")),
        }
    }
    println!(
        "{} maps: {ok} decode ({tiles} terrain cells, {overlays} with overlays, {previews} with previews), {} failed",
        maps.len(),
        failures.len()
    );
    println!("  by theater: {by_theater:?}");
    println!("  bytes after the last whole iso tile: {trailing:?}");
    println!(
        "  cells past the theater's tile count: {out_of_range}; in Marble Madness sets without files: {marble}; \
         other cells whose tile file is absent: {missing}"
    );
    for (k, v) in &missing_sets {
        println!("    {v:>6} {k}");
    }
    for f in failures.iter().take(20) {
        println!("  {f}");
    }
    Ok(failures.is_empty())
}

/// Write a map's embedded preview.
pub(crate) fn preview(
    game: Game,
    game_dir: Option<&Path>,
    file: &str,
    out: &Path,
) -> io::Result<bool> {
    let vfs = vfs(game, game_dir.map(Path::to_path_buf))?;
    let m = Map::from_ini(&Ini::parse(&read(&vfs, file)?))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let Some((r, px)) = m.preview else {
        println!("{file} has no preview");
        return Ok(false);
    };
    let rgba: Vec<u8> = px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect();
    write_rgba(
        out,
        u32::try_from(r.width).unwrap_or(0),
        u32::try_from(r.height).unwrap_or(0),
        &rgba,
    )?;
    println!(
        "{file}: {} {}x{}, {} cells; preview {}x{} written to {}",
        m.theater,
        m.size.width,
        m.size.height,
        m.tiles.len(),
        r.width,
        r.height,
        out.display()
    );
    Ok(true)
}
