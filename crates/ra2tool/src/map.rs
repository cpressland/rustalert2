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

/// Summarise a map: name, theater, sizes and what it contains.
pub(crate) fn info(game: Game, game_dir: Option<&Path>, file: &str) -> io::Result<bool> {
    let vfs = vfs(game, game_dir.map(Path::to_path_buf))?;
    let ini = Ini::parse(&read(&vfs, file)?);
    let m = Map::from_ini(&ini)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let count = |section: &str| {
        ini.section(section)
            .map_or(0, ra2_formats::ini::Section::len)
    };
    println!(
        "{file}: {}",
        ini.get("Basic", "Name").unwrap_or("(no name)")
    );
    println!(
        "  theater {}, size {}x{}, local {:?}",
        m.theater,
        m.size.width,
        m.size.height,
        m.local_size.map(|r| (r.x, r.y, r.width, r.height))
    );
    println!(
        "  {} terrain cells, {} overlay cells, preview {}",
        m.tiles.len(),
        m.overlay
            .as_ref()
            .map_or(0, |o| o.iter().filter(|&&v| v != 0xff).count()),
        m.preview
            .as_ref()
            .map_or("none".to_string(), |(r, _)| format!(
                "{}x{}",
                r.width, r.height
            ))
    );
    println!(
        "  {} houses, {} units, {} infantry, {} aircraft, {} structures, {} terrain objects, {} waypoints, {} triggers",
        count("Houses"),
        count("Units"),
        count("Infantry"),
        count("Aircraft"),
        count("Structures"),
        count("Terrain"),
        count("Waypoints"),
        count("Triggers")
    );
    Ok(true)
}

fn load_view(
    game: Game,
    game_dir: Option<&Path>,
    file: &str,
) -> io::Result<(ra2_vfs::mount::Vfs, ra2_render::compose::MapView)> {
    let mut vfs = vfs(game, game_dir.map(Path::to_path_buf))?;
    let (rules, art) = if game == Game::YurisRevenge {
        ("rulesmd.ini", "artmd.ini")
    } else {
        ("rules.ini", "art.ini")
    };
    // A path to a file on disk is read from there; anything else is a name
    // in the game's file system.
    let view = if Path::new(file).is_file() {
        let data = std::fs::read(file)?;
        ra2_render::compose::MapView::from_data(&mut vfs, file, &data, rules, art)?
    } else {
        ra2_render::compose::MapView::load(&mut vfs, file, rules, art)?
    };
    Ok((vfs, view))
}

/// Open a map in a window.
pub(crate) fn view(
    game: Game,
    game_dir: Option<&Path>,
    file: &str,
    shroud: bool,
    snapshot: Option<&Path>,
) -> io::Result<bool> {
    use ra2_render::compose::Layers;
    let (vfs, mut view) = load_view(game, game_dir, file)?;
    if let Some(out) = snapshot {
        let (canvas, _) = view.compose(
            &vfs,
            Layers {
                shroud,
                ..Layers::default()
            },
            None,
        );
        let (w, h) = (1280, 800);
        let px = ra2_render::viewer::snapshot(
            &canvas,
            w,
            h,
            None,
            ra2_render::viewer::Scaling::default(),
        )
        .map_err(io::Error::other)?;
        write_rgba(out, w, h, &px)?;
        println!("{file}: {w}x{h} GPU snapshot written to {}", out.display());
        return Ok(true);
    }
    let title = format!("{file} ({})", view.map.theater);
    ra2_render::viewer::run(
        &title,
        Layers {
            shroud,
            ..Layers::default()
        },
        |layers| view.compose(&vfs, layers, None).0,
    )
    .map_err(io::Error::other)?;
    Ok(true)
}

/// Draw a map to an image: terrain, objects, lighting and (optionally) the
/// player's starting shroud.
/// Draw every map the game ships (the M3 exit check): each through the
/// whole composition, shrouded where the map names a player, optionally
/// written as a small PNG.
/// Returns whether every map drew.
pub(crate) fn render_all(
    game: Game,
    game_dir: Option<&Path>,
    scale: u32,
    out: Option<&Path>,
) -> io::Result<bool> {
    use ra2_render::compose::{Layers, MapView};
    let (rules, art) = if game == Game::YurisRevenge {
        ("rulesmd.ini", "artmd.ini")
    } else {
        ("rules.ini", "art.ini")
    };
    let maps = all_maps(game, game_dir)?;
    let mut missing: BTreeMap<String, usize> = BTreeMap::new();
    let (mut drawn, mut failed) = (0usize, Vec::new());
    let start = std::time::Instant::now();
    for (label, data) in &maps {
        // Each map mounts its own theater, so each gets a fresh file system.
        let mut vfs = vfs(game, game_dir.map(Path::to_path_buf))?;
        let t0 = std::time::Instant::now();
        let mut view = match MapView::from_data(&mut vfs, label, data, rules, art) {
            Ok(v) => v,
            Err(e) => {
                failed.push(format!("{label}: {e}"));
                continue;
            }
        };
        // Shroud only where the map names the player's house: multiplayer
        // maps have none until a game starts.
        let shroud = view.ini.get("Basic", "Player").is_some();
        let (canvas, stats) = view.compose(
            &vfs,
            Layers {
                shroud,
                ..Layers::default()
            },
            None,
        );
        drawn += 1;
        for (name, n) in &stats.scene.missing {
            *missing.entry(name.clone()).or_default() += n;
        }
        if let Some(dir) = out {
            std::fs::create_dir_all(dir)?;
            let s = scale.max(1) as usize;
            let (w, h) = (canvas.width as usize / s, canvas.height as usize / s);
            let mut small = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let i = ((y * s) * canvas.width as usize + x * s) * 4;
                    small.extend_from_slice(&canvas.rgba[i..i + 4]);
                }
            }
            let name: String = label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            write_rgba(
                &dir.join(format!("{name}.png")),
                u32::try_from(w).expect("fits"),
                u32::try_from(h).expect("fits"),
                &small,
            )?;
        }
        println!(
            "{label}: {} {}x{}, {} revealed, {:.0} ms",
            view.map.theater,
            view.map.size.width,
            view.map.size.height,
            stats.revealed,
            t0.elapsed().as_secs_f64() * 1000.0
        );
    }
    let mut missing: Vec<_> = missing.into_iter().collect();
    missing.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    println!(
        "{drawn} of {} maps drawn in {:.1} s; {} failed; most-missed art {:?}",
        maps.len(),
        start.elapsed().as_secs_f64(),
        failed.len(),
        missing.iter().take(12).collect::<Vec<_>>()
    );
    for f in &failed {
        println!("  {f}");
    }
    Ok(failed.is_empty())
}

pub(crate) fn render(
    game: Game,
    game_dir: Option<&Path>,
    file: &str,
    scale: u32,
    around: Option<(i16, i16, u32, u32)>,
    shroud: bool,
    out: &Path,
) -> io::Result<bool> {
    use ra2_render::compose::Layers;
    let (vfs, mut view) = load_view(game, game_dir, file)?;
    let rect = around.map(|(cx, cy, w, h)| {
        let p = ra2_sim::coords::Cell::new(cx, cy).screen(0);
        let (w, h) = (
            i32::try_from(w).unwrap_or(800),
            i32::try_from(h).unwrap_or(600),
        );
        (p.x - w / 2, p.y - h / 2, p.x + w / 2, p.y + h / 2)
    });
    let (canvas, stats) = view.compose(
        &vfs,
        Layers {
            shroud,
            ..Layers::default()
        },
        rect,
    );
    let s = scale.max(1) as usize;
    let (w, h) = (canvas.width as usize / s, canvas.height as usize / s);
    let mut small = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * s) * canvas.width as usize + x * s) * 4;
            small.extend_from_slice(&canvas.rgba[i..i + 4]);
        }
    }
    write_rgba(
        out,
        u32::try_from(w).expect("fits"),
        u32::try_from(h).expect("fits"),
        &small,
    )?;
    let (t, o) = (&stats.terrain, &stats.scene);
    println!(
        "{file}: {} {}x{}; {} cells ({} Marble Madness, {} cleared); {} overlays, {} terrain objects, {} smudges, \
         {} buildings, {} infantry, {} vehicles; missing art {:?}; {}x{} written to {}",
        view.map.theater,
        view.map.size.width,
        view.map.size.height,
        t.cells,
        t.marble_madness,
        t.cleared,
        o.overlays,
        o.terrain_objects,
        o.smudges,
        o.buildings,
        o.infantry,
        o.vehicles,
        o.missing.iter().take(8).collect::<Vec<_>>(),
        w,
        h,
        out.display()
    );
    Ok(true)
}
