//! `ra2tool vfs`: the game's file system as the game would mount it.

use std::io;
use std::path::PathBuf;

use ra2_vfs::install::{Game, Inspection, SearchContext};
use ra2_vfs::mount::{Mounted, Vfs};

fn mounted(game: Game, game_dir: Option<PathBuf>) -> io::Result<(Vfs, Vec<Mounted>)> {
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
    let results = vfs.mount_startup(game);
    Ok((vfs, results))
}

/// Print the startup mount sequence and what each step did.
pub(crate) fn mounts(game: Game, game_dir: Option<PathBuf>) -> io::Result<bool> {
    let (vfs, results) = mounted(game, game_dir)?;
    let mut ok = true;
    for r in &results {
        match r {
            Mounted::Yes { name, from } => {
                let n = vfs
                    .mounts()
                    .iter()
                    .position(|m| &m.name == name)
                    .unwrap_or_default();
                println!("#{n:<3} {name:<16} {from}");
            }
            Mounted::Absent { name } => println!("     {name:<16} (not present)"),
            Mounted::Duplicate { name } => println!("     {name:<16} (already mounted)"),
            Mounted::Failed { name, error } => {
                ok = false;
                println!("FAIL {name:<16} {error}");
            }
        }
    }
    println!(
        "{} archives mounted for {}",
        vfs.mounts().len(),
        game.name()
    );
    Ok(ok)
}

/// Print where each name resolves.
pub(crate) fn which(game: Game, game_dir: Option<PathBuf>, names: &[String]) -> io::Result<bool> {
    let (vfs, _) = mounted(game, game_dir)?;
    let mut all = true;
    for name in names {
        if let Some(loc) = vfs.locate(name) {
            println!("{name}: {loc}");
        } else {
            all = false;
            println!("{name}: not found");
        }
    }
    Ok(all)
}
