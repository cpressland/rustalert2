//! `ra2tool pal`: palettes and house colours.

use std::io;
use std::path::{Path, PathBuf};

use ra2_formats::ini::Ini;
use ra2_formats::pal::{Hsv, Palette, Rgb, house_ramp, remap};
use ra2_vfs::install::{Game, Inspection, SearchContext};
use ra2_vfs::mount::Vfs;

use crate::image::write_rgba;

pub(crate) fn vfs(game: Game, game_dir: Option<PathBuf>) -> io::Result<Vfs> {
    let root = match game_dir {
        Some(d) => d,
        None => {
            SearchContext::from_env()
                .find(game)
                .map_err(io::Error::other)?
                .inspection
                .root
        }
    };
    let mut vfs = Vfs::new(Inspection::of(&root)?);
    vfs.mount_startup(game);
    Ok(vfs)
}

pub(crate) fn read(vfs: &Vfs, name: &str) -> io::Result<Vec<u8>> {
    vfs.read(name)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{name}: not found")))
}

pub(crate) fn palette(vfs: &Vfs, name: &str) -> io::Result<Palette> {
    Palette::parse(&read(vfs, name)?).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{name}: not a 768-byte palette"),
        )
    })
}

/// A house colour by `[Colors]` name in the rules, or a literal `H,S,V`.
pub(crate) fn colour(vfs: &Vfs, game: Game, spec: &str) -> io::Result<Hsv> {
    if spec.contains(',') {
        return Ok(Hsv::parse(spec));
    }
    let rules = if game == Game::YurisRevenge {
        "rulesmd.ini"
    } else {
        "rules.ini"
    };
    let ini = Ini::parse(&read(vfs, rules)?);
    ini.get("Colors", spec)
        .map(Hsv::parse)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{spec}: not in [Colors]")))
}

/// Draw colours as a grid of square swatches.
fn swatches(path: &Path, colors: &[Rgb], per_row: usize) -> io::Result<()> {
    const CELL: usize = 16;
    let rows = colors.len().div_ceil(per_row);
    let (w, h) = (per_row * CELL, rows * CELL);
    let mut rgba = vec![0u8; w * h * 4];
    for (i, c) in colors.iter().enumerate() {
        let (cx, cy) = ((i % per_row) * CELL, (i / per_row) * CELL);
        for y in cy..cy + CELL {
            for x in cx..cx + CELL {
                let p = (y * w + x) * 4;
                rgba[p..p + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    write_rgba(
        path,
        u32::try_from(w).expect("small"),
        u32::try_from(h).expect("small"),
        &rgba,
    )
}

/// Write a palette's 256 colours, optionally with a house colour applied.
pub(crate) fn png(
    game: Game,
    game_dir: Option<PathBuf>,
    file: &str,
    house: Option<&str>,
    out: &Path,
) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let mut colors = palette(&vfs, file)?.to_rgb8();
    if let Some(spec) = house {
        colors = remap(&colors, colour(&vfs, game, spec)?);
    }
    swatches(out, &colors, 16)?;
    println!("wrote {}", out.display());
    Ok(true)
}

/// Print, and optionally draw, the ramps for house colours.
pub(crate) fn ramps(
    game: Game,
    game_dir: Option<PathBuf>,
    specs: &[String],
    out: Option<&Path>,
) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let specs: Vec<String> = if specs.is_empty() {
        let rules = if game == Game::YurisRevenge {
            "rulesmd.ini"
        } else {
            "rules.ini"
        };
        let ini = Ini::parse(&read(&vfs, rules)?);
        ini.section("Colors")
            .map(|s| s.entries().map(|(k, _)| k.to_string()).collect())
            .unwrap_or_default()
    } else {
        specs.to_vec()
    };
    let mut all = Vec::new();
    for spec in &specs {
        let hsv = colour(&vfs, game, spec)?;
        let ramp = house_ramp(hsv);
        let hex: Vec<String> = ramp
            .iter()
            .map(|c| format!("{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
            .collect();
        println!(
            "{spec:<16} {},{},{}  {}",
            hsv.h,
            hsv.s,
            hsv.v,
            hex.join(" ")
        );
        all.extend_from_slice(&ramp);
    }
    if let Some(path) = out {
        swatches(path, &all, 16)?;
        println!("wrote {}", path.display());
    }
    Ok(true)
}
