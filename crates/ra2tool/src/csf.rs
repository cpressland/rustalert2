//! `ra2tool csf`: read the game's string tables.

use std::io;
use std::path::PathBuf;

use ra2_formats::csf::StringTable;
use ra2_vfs::install::{Game, Inspection, SearchContext};
use ra2_vfs::mount::Vfs;

fn table(game: Game, game_dir: Option<PathBuf>, file: &str) -> io::Result<StringTable> {
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
    let data = vfs
        .read(file)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{file}: not found")))?;
    StringTable::parse(&data)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{file}: {e}")))
}

/// Print labels' text; with `--grep`, labels whose text contains a string.
pub(crate) fn get(
    game: Game,
    game_dir: Option<PathBuf>,
    file: &str,
    labels: &[String],
    grep: Option<&str>,
) -> io::Result<bool> {
    let t = table(game, game_dir, file)?;
    if let Some(needle) = grep {
        let needle = needle.to_lowercase();
        for e in t
            .entries()
            .iter()
            .filter(|e| e.text.to_lowercase().contains(&needle))
        {
            println!("{}: {}", e.label, e.text.replace('\n', "\\n"));
        }
        return Ok(true);
    }
    let mut all = true;
    for l in labels {
        all &= t.entry(l).is_some();
        println!("{l}: {}", t.text(l).replace('\n', "\\n"));
    }
    Ok(all)
}

/// Summarise a table.
pub(crate) fn stats(game: Game, game_dir: Option<PathBuf>, file: &str) -> io::Result<bool> {
    let t = table(game, game_dir, file)?;
    let extras = t.entries().iter().filter(|e| e.extra.is_some()).count();
    let alternates = t
        .entries()
        .iter()
        .filter(|e| !e.alternates.is_empty())
        .count();
    let longest = t.entries().iter().map(|e| e.label.len()).max().unwrap_or(0);
    println!(
        "{file}: version {}, {:?}, {} labels, {extras} with extra values, {alternates} with more than one string, \
         longest label {longest} bytes, {} repeated labels",
        t.version,
        t.language,
        t.entries().len(),
        t.duplicates.len()
    );
    for d in t.duplicates.iter().take(10) {
        println!("    repeated: {d}");
    }
    Ok(true)
}
