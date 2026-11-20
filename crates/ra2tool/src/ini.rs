//! `ra2tool ini`: read INIs from the game's file system the way the game does.

use std::io;
use std::path::PathBuf;

use ra2_formats::ini::{Duplicate, Ini, MAX_LINE};
use ra2_vfs::install::{Game, Inspection, SearchContext};
use ra2_vfs::mount::Vfs;

fn vfs(game: Game, game_dir: Option<PathBuf>) -> io::Result<Vfs> {
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

fn load(vfs: &Vfs, name: &str) -> io::Result<(Vec<u8>, Ini)> {
    let data = vfs
        .read(name)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{name}: not found")))?;
    let ini = Ini::parse(&data);
    Ok((data, ini))
}

/// Print one value, or a whole section.
pub(crate) fn get(
    game: Game,
    game_dir: Option<PathBuf>,
    file: &str,
    section: &str,
    key: Option<&str>,
) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    let (_, ini) = load(&vfs, file)?;
    let Some(s) = ini.section(section) else {
        println!("[{section}] not in {file}");
        return Ok(false);
    };
    match key {
        Some(k) => {
            let Some(v) = s.get(k) else {
                println!("{k} not in [{section}]");
                return Ok(false);
            };
            println!("{v}");
        }
        None => {
            for (k, v) in s.entries() {
                println!("{k}={v}");
            }
        }
    }
    Ok(true)
}

/// Summarise how files parse: sections, entries, duplicates, long lines.
pub(crate) fn stats(game: Game, game_dir: Option<PathBuf>, files: &[String]) -> io::Result<bool> {
    let vfs = vfs(game, game_dir)?;
    for name in files {
        let (data, ini) = match load(&vfs, name) {
            Ok(x) => x,
            Err(e) => {
                println!("{name}: {e}");
                continue;
            }
        };
        let entries: usize = ini
            .sections()
            .iter()
            .map(ra2_formats::ini::Section::len)
            .sum();
        let long = data
            .split(|&b| b == b'\n')
            .filter(|l| l.len() > MAX_LINE)
            .count();
        let (mut dup_sections, mut dup_keys) = (0, 0);
        for d in &ini.duplicates {
            match d {
                Duplicate::Section(_) => dup_sections += 1,
                Duplicate::Key { .. } => dup_keys += 1,
            }
        }
        println!(
            "{name}: {} bytes, {} sections, {entries} entries, {dup_sections} repeated sections, \
             {dup_keys} repeated keys, {long} lines over {MAX_LINE} bytes",
            data.len(),
            ini.sections().len()
        );
        for d in ini.duplicates.iter().take(10) {
            match d {
                Duplicate::Section(s) => println!("    repeated section [{s}]"),
                Duplicate::Key { section, key } => println!("    repeated key [{section}] {key}"),
            }
        }
    }
    Ok(true)
}
