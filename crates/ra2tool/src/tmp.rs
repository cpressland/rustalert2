//! `ra2tool tmp`: theaters and their terrain tiles.

use std::io;
use std::path::{Path, PathBuf};

use ra2_formats::ini::Ini;
use ra2_formats::pal::Palette;
use ra2_formats::tmp::{Tmp, diamond_rows, tile_sets};
use ra2_vfs::install::Game;
use ra2_vfs::mount::{Mounted, Vfs};
use ra2_vfs::theater::Theater;

use crate::image::write_rgba;
use crate::pal::{read, vfs};

fn theater_vfs(
    game: Game,
    game_dir: Option<PathBuf>,
    t: Theater,
) -> io::Result<(Vfs, Vec<Mounted>)> {
    let mut v = vfs(game, game_dir)?;
    let r = v.mount_theater(t);
    Ok((v, r))
}

pub(crate) fn parse_theater(name: &str) -> io::Result<Theater> {
    Theater::from_name(name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name}: not a theater"),
        )
    })
}

/// Mount each theater, read its INI and parse every tile it lists.
pub(crate) fn check(game: Game, game_dir: Option<&Path>) -> io::Result<bool> {
    let mut ok = true;
    for t in Theater::ALL {
        let (vfs, mounted) = theater_vfs(game, game_dir.map(Path::to_path_buf), t)?;
        let names: Vec<String> = mounted
            .iter()
            .map(|m| match m {
                Mounted::Yes { name, .. } => name.clone(),
                Mounted::Absent { name } => format!("({name} absent)"),
                Mounted::Duplicate { name } => format!("({name} again)"),
                Mounted::Failed { name, error } => format!("({name} FAILED: {error})"),
            })
            .collect();
        let Some(ini) = vfs.read(&t.ini())? else {
            println!("{:<9} {}: no {}", t.info().name, names.join(" "), t.ini());
            continue;
        };
        let sets = tile_sets(&Ini::parse(&ini));
        let (mut files, mut missing, mut cells, mut extras, mut zs) = (0, 0, 0, 0, 0);
        let (mut editor_missing, mut missing_names) = (0, Vec::new());
        let mut failures = Vec::new();
        for set in &sets {
            for n in 1..=set.tiles {
                let file = format!("{}{n:02}.{}", set.file_name, t.ext());
                let Some(data) = vfs.read(&file)? else {
                    if set.is_editor_only() {
                        editor_missing += 1;
                    } else {
                        missing += 1;
                        missing_names.push(file);
                    }
                    continue;
                };
                match Tmp::parse(&data) {
                    Ok(tmp) => {
                        files += 1;
                        for c in tmp.cells.iter().flatten() {
                            cells += 1;
                            extras += usize::from(c.extra.is_some());
                            zs += usize::from(c.z.is_some());
                        }
                    }
                    Err(e) => failures.push(format!("{file}: {e}")),
                }
            }
        }
        let tiles: u32 = sets.iter().map(|s| s.tiles).sum();
        println!(
            "{:<9} {}\n          {} sets, {tiles} tiles: {files} parse ({cells} cells, {extras} with extra art, {zs} with depth), \
             {editor_missing} absent from editor-only sets, {missing} missing from game sets, {} failed",
            t.info().name,
            names.join(" "),
            sets.len(),
            failures.len()
        );
        for f in failures.iter().chain(&missing_names).take(10) {
            println!("    {f}");
        }
        ok &= failures.is_empty();
    }
    Ok(ok)
}

/// Draw a tile template, its cells placed as the file places them.
pub(crate) fn png(
    game: Game,
    game_dir: Option<PathBuf>,
    theater: &str,
    file: &str,
    out: &Path,
) -> io::Result<bool> {
    let t = parse_theater(theater)?;
    let (vfs, _) = theater_vfs(game, game_dir, t)?;
    let tmp = Tmp::parse(&read(&vfs, file)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let pal = Palette::parse(&read(&vfs, &t.palettes()[2])?)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "bad palette"))?
        .to_rgb8();
    // Bounds of every cell and extra.
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    let tw = i32::try_from(tmp.tile_width).expect("small");
    let th = i32::try_from(tmp.tile_height).expect("small");
    for c in tmp.cells.iter().flatten() {
        let y = c.y - i32::from(c.height) * 15;
        (x0, y0, x1, y1) = (x0.min(c.x), y0.min(y), x1.max(c.x + tw), y1.max(y + th));
        if let Some(e) = &c.extra {
            let ey = e.y - i32::from(c.height) * 15;
            let (ew, eh) = (
                i32::try_from(e.width).expect("small"),
                i32::try_from(e.height).expect("small"),
            );
            (x0, y0, x1, y1) = (x0.min(e.x), y0.min(ey), x1.max(e.x + ew), y1.max(ey + eh));
        }
    }
    let (w, h) = (
        usize::try_from(x1 - x0).unwrap_or(1),
        usize::try_from(y1 - y0).unwrap_or(1),
    );
    let mut rgba = vec![0u8; w * h * 4];
    let mut plot = |x: i32, y: i32, i: u8| {
        if i == 0 {
            return;
        }
        let (px, py) = (
            usize::try_from(x - x0).unwrap_or(0),
            usize::try_from(y - y0).unwrap_or(0),
        );
        if px < w && py < h {
            let c = pal[usize::from(i)];
            rgba[(py * w + px) * 4..(py * w + px) * 4 + 4]
                .copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    };
    let rows = diamond_rows(tmp.tile_width, tmp.tile_height);
    for c in tmp.cells.iter().flatten() {
        let lift = i32::from(c.height) * 15;
        let mut k = 0;
        for (ry, (start, len)) in rows.iter().enumerate() {
            for dx in 0..*len {
                let x = c.x + i32::try_from(start + dx).expect("small");
                plot(
                    x,
                    c.y - lift + i32::try_from(ry).expect("small"),
                    c.image[k],
                );
                k += 1;
            }
        }
        if let Some(e) = &c.extra {
            let ew = usize::try_from(e.width).expect("small");
            for (k, &i) in e.image.iter().enumerate() {
                let (ex, ey) = (
                    i32::try_from(k % ew).expect("small"),
                    i32::try_from(k / ew).expect("small"),
                );
                plot(e.x + ex, e.y - lift + ey, i);
            }
        }
    }
    write_rgba(
        out,
        u32::try_from(w).expect("small"),
        u32::try_from(h).expect("small"),
        &rgba,
    )?;
    println!(
        "{file}: {}x{} cells; wrote {}",
        tmp.width,
        tmp.height,
        out.display()
    );
    Ok(true)
}
