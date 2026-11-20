//! `ra2tool font`: the game's bitmap font.

use std::io;
use std::path::{Path, PathBuf};

use ra2_formats::fnt::Font;
use ra2_vfs::install::Game;

use crate::image::write_rgba;
use crate::pal::{read, vfs};

/// Describe the font and check every glyph it maps.
pub(crate) fn check(game: Game, game_dir: Option<PathBuf>) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let data = read(&vfs, "game.fnt")?;
    let f = Font::parse(&data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let mut widest = 0;
    let mut bad = 0;
    for c in (0u32..65_536).filter_map(char::from_u32) {
        if let Some(i) = f.index(c) {
            match f.glyph(i) {
                Some(g) => widest = widest.max(g.width),
                None => bad += 1,
            }
        }
    }
    println!(
        "game.fnt: {} glyphs, {} characters mapped, {} rows of {} bytes, line height {}, widest {} (header says {}); {bad} bad indices",
        f.glyph_count,
        f.mapped(),
        f.lines,
        f.stride,
        f.height,
        widest,
        f.max_width
    );
    Ok(bad == 0)
}

/// Render a line of text.
pub(crate) fn render(
    game: Game,
    game_dir: Option<PathBuf>,
    text: &str,
    out: &Path,
) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let data = read(&vfs, "game.fnt")?;
    let f = Font::parse(&data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let glyphs: Vec<_> = text
        .chars()
        .map(|c| f.index(c).and_then(|i| f.glyph(i)))
        .collect();
    let w: usize = glyphs
        .iter()
        .map(|g| g.as_ref().map_or(4, |g| usize::from(g.width)))
        .sum::<usize>()
        + 4;
    let h = usize::try_from(f.height).unwrap_or(16) + 4;
    let lines = usize::try_from(f.lines).unwrap_or(16);
    let mut rgba = vec![0u8; w * h * 4];
    for px in rgba.chunks_mut(4) {
        px.copy_from_slice(&[24, 24, 24, 255]);
    }
    let mut x0 = 2;
    for g in glyphs.iter().flatten() {
        let gw = usize::from(g.width);
        for y in 0..lines {
            for x in 0..gw {
                if g.mask[y * gw + x] {
                    let p = ((y + 2) * w + x0 + x) * 4;
                    rgba[p..p + 4].copy_from_slice(&[255, 214, 0, 255]);
                }
            }
        }
        x0 += gw;
    }
    write_rgba(
        out,
        u32::try_from(w).expect("small"),
        u32::try_from(h).expect("small"),
        &rgba,
    )?;
    println!("wrote {}", out.display());
    Ok(true)
}

/// Decode every PCX in the mounted game.
pub(crate) fn check_pcx(game: Game, game_dir: Option<PathBuf>) -> io::Result<bool> {
    use ra2_formats::pcx::{Image, decode};
    let vfs = vfs(game, game_dir)?;
    let (mut indexed, mut rgb) = (0, 0);
    let mut failures = Vec::new();
    for m in vfs.mounts() {
        for e in &m.archive.index().entries {
            let Ok(data) = m.archive.read(*e) else {
                continue;
            };
            if data.len() < 128 || data[0] != 0x0a || data[1] != 5 || data[2] != 1 {
                continue;
            }
            match decode(&data) {
                Ok(Image::Indexed { .. }) => indexed += 1,
                Ok(Image::Rgb { .. }) => rgb += 1,
                Err(err) => failures.push(format!("{} {:08X}: {err}", m.name, e.id)),
            }
        }
    }
    println!(
        "{indexed} paletted and {rgb} RGB PCX images decode; {} failed",
        failures.len()
    );
    for f in failures.iter().take(10) {
        println!("  {f}");
    }
    Ok(failures.is_empty())
}
