//! `ra2tool shp`: decode SHP sprites.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use ra2_formats::pal::{Hsv, remap};
use ra2_formats::shp::{Compression, Shp};
use ra2_vfs::install::Game;

use crate::image::write_rgba;
use crate::pal::{colour, palette, read, vfs};

/// Draw frames side by side in palette colours (index 0 transparent).
pub(crate) fn png(
    game: Game,
    game_dir: Option<PathBuf>,
    file: &str,
    pal_name: &str,
    house: Option<&str>,
    frames: Option<(usize, usize)>,
    out: &Path,
) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let data = read(&vfs, file)?;
    let shp =
        Shp::parse(&data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
    let mut colors = palette(&vfs, pal_name)?.to_rgb8();
    if let Some(h) = house {
        let hsv: Hsv = colour(&vfs, game, h)?;
        colors = remap(&colors, hsv);
    }
    let (first, count) = frames.unwrap_or((0, shp.frames.len()));
    let count = count.min(shp.frames.len().saturating_sub(first));
    let (cw, ch) = (usize::from(shp.width), usize::from(shp.height));
    let per_row = count.clamp(1, 16);
    let rows = count.div_ceil(per_row).max(1);
    let (w, h) = (cw * per_row, ch * rows);
    let mut rgba = vec![0u8; w * h * 4];
    for k in 0..count {
        let canvas = shp
            .frame_on_canvas(first + k)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;
        let (ox, oy) = ((k % per_row) * cw, (k / per_row) * ch);
        for y in 0..ch {
            for x in 0..cw {
                let i = canvas[y * cw + x];
                if i != 0 {
                    let c = colors[usize::from(i)];
                    let p = ((oy + y) * w + ox + x) * 4;
                    rgba[p..p + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                }
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
        "{file}: {}x{}, {} frames; wrote {}",
        shp.width,
        shp.height,
        shp.frames.len(),
        out.display()
    );
    Ok(true)
}

/// Try every file in every mounted archive as an SHP, and decode every
/// frame of those that parse.
pub(crate) fn check(game: Game, game_dir: Option<PathBuf>) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let (mut entries, mut parsed, mut frames) = (0usize, 0usize, 0usize);
    let mut flags: BTreeMap<u8, usize> = BTreeMap::new();
    let mut failures = Vec::new();
    for m in vfs.mounts() {
        for e in &m.archive.index().entries {
            entries += 1;
            let Ok(data) = m.archive.read(*e) else {
                continue;
            };
            let Ok(shp) = Shp::parse(&data) else { continue };
            parsed += 1;
            for (k, h) in shp.frames.iter().enumerate() {
                if !h.is_empty() {
                    *flags.entry(h.flags).or_default() += 1;
                }
                match shp.frame(k) {
                    Ok(_) => frames += 1,
                    Err(err) => {
                        failures.push(format!("{} {:08X}: {err}", m.name, e.id));
                        break;
                    }
                }
            }
        }
    }
    println!(
        "{entries} files in {} archives; {parsed} parse as SHP; {frames} frames decoded",
        vfs.mounts().len()
    );
    for (f, n) in &flags {
        let c = ra2_formats::shp::FrameHeader {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
            flags: *f,
            radar_color: [0; 4],
            offset: 1,
        }
        .compression();
        let name = match c {
            Compression::Raw => "raw",
            Compression::Rows => "rows",
            Compression::RowsZeroRuns => "rows with zero runs",
        };
        println!("  flags {f}: {n} frames ({name})");
    }
    println!("{} files parsed but failed to decode", failures.len());
    for f in failures.iter().take(20) {
        println!("  {f}");
    }
    Ok(failures.is_empty())
}
