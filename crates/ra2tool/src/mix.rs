//! `ra2tool mix`: list, extract and check MIX archives.

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use ra2_vfs::archive::Archive;
use ra2_vfs::install::{Game, Inspection, SearchContext};
use ra2_vfs::names;

/// Open an archive by path. The path is a file on disk, or a file in the game
/// install (case-insensitive), followed by archives nested inside it:
/// `ra2md.mix/localmd.mix`.
pub(crate) fn open(path: &str) -> io::Result<Archive> {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    // The longest prefix that is a file on disk.
    let mut split = None;
    for i in (1..=parts.len()).rev() {
        let candidate: PathBuf = if path.starts_with('/') {
            Path::new("/").join(parts[..i].join("/"))
        } else {
            PathBuf::from(parts[..i].join("/"))
        };
        if candidate.is_file() {
            split = Some((candidate, i));
            break;
        }
    }
    let (file, used) = if let Some(s) = split {
        s
    } else {
        let install = SearchContext::from_env()
            .find(Game::RedAlert2)
            .map_err(io::Error::other)?;
        let file = install
            .inspection
            .file(parts.first().copied().unwrap_or_default())
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{path}: not found")))?;
        (file, 1)
    };
    let mut archive =
        Archive::open(&file).map_err(|e| annotate(&e, &file.display().to_string()))?;
    for name in &parts[used..] {
        let entry = archive.find(name).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{name}: not in the archive"),
            )
        })?;
        archive = archive.open_nested(entry).map_err(|e| annotate(&e, name))?;
    }
    Ok(archive)
}

fn annotate(e: &io::Error, what: &str) -> io::Error {
    io::Error::new(e.kind(), format!("{what}: {e}"))
}

fn names_for(archive: &Archive, extra: Option<&Path>) -> io::Result<BTreeMap<u32, String>> {
    let mut names = names::known();
    for n in archive.local_names() {
        names.insert(ra2_formats::mix::mix_id(&n), n);
    }
    if let Some(list) = extra {
        for n in std::fs::read_to_string(list)?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
        {
            names.insert(ra2_formats::mix::mix_id(n), n.to_string());
        }
    }
    Ok(names)
}

pub(crate) fn ls(path: &str, names_file: Option<&Path>) -> io::Result<()> {
    let archive = open(path)?;
    let names = names_for(&archive, names_file)?;
    let idx = archive.index();
    let mut out = io::stdout().lock();
    writeln!(
        out,
        "{} files, body {} bytes, {}{}",
        idx.entries.len(),
        idx.body_size,
        if idx.is_encrypted() {
            "encrypted"
        } else {
            "plain"
        },
        if idx.has_checksum() { ", SHA-1" } else { "" }
    )?;
    let mut entries = idx.entries.clone();
    entries.sort_by_key(|e| e.offset);
    for e in entries {
        let name = names.get(&e.id).map_or("?", String::as_str);
        writeln!(out, "{:08X} {:>10} {:>10}  {name}", e.id, e.offset, e.size)?;
    }
    Ok(())
}

pub(crate) fn extract(path: &str, files: &[String], out_dir: Option<&Path>) -> io::Result<()> {
    let archive = open(path)?;
    for name in files {
        let data = archive.read_file(name)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{name}: not in the archive"),
            )
        })?;
        match out_dir {
            Some(dir) => {
                std::fs::create_dir_all(dir)?;
                std::fs::write(dir.join(name), &data)?;
                eprintln!("{name}: {} bytes", data.len());
            }
            None => match io::stdout().lock().write_all(&data) {
                // `| head` closing the pipe is not an error.
                Err(e) if e.kind() == io::ErrorKind::BrokenPipe => return Ok(()),
                other => other?,
            },
        }
    }
    Ok(())
}

/// Open every archive in the install and every known archive nested in
/// them; check their bounds and checksums. Returns whether all were fine.
pub(crate) fn check(game_dir: Option<PathBuf>) -> io::Result<bool> {
    let root = match game_dir {
        Some(d) => d,
        None => {
            SearchContext::from_env()
                .find(Game::RedAlert2)
                .map_err(io::Error::other)?
                .inspection
                .root
        }
    };
    let inspection = Inspection::of(&root)?;
    let known = names::known();
    let mut ok = true;
    let mut totals = (0usize, 0usize);
    for (lower, real) in &inspection.files {
        let ext = lower.rsplit('.').next().unwrap_or_default();
        if !matches!(ext, "mix" | "mmx" | "yro") {
            continue;
        }
        match Archive::open(&root.join(real)) {
            Ok(a) => ok &= check_tree(&a, real, &known, 0, &mut totals),
            Err(e) => {
                println!("FAIL {real}: {e}");
                ok = false;
            }
        }
    }
    println!(
        "{} archives, {} files indexed: {}",
        totals.0,
        totals.1,
        if ok { "all ok" } else { "FAILURES" }
    );
    Ok(ok)
}

fn check_tree(
    a: &Archive,
    label: &str,
    known: &BTreeMap<u32, String>,
    depth: usize,
    totals: &mut (usize, usize),
) -> bool {
    totals.0 += 1;
    totals.1 += a.index().entries.len();
    let checksum = match a.verify_checksum() {
        Ok(None) => "",
        Ok(Some(true)) => ", SHA-1 ok",
        Ok(Some(false)) => ", SHA-1 MISMATCH",
        Err(_) => ", SHA-1 unreadable",
    };
    let mut ok = !checksum.contains("MISMATCH") && !checksum.contains("unreadable");
    println!(
        "{}{} {label}: {} files{}{checksum}",
        if ok { "ok   " } else { "FAIL " },
        "  ".repeat(depth),
        a.index().entries.len(),
        if a.index().is_encrypted() {
            ", encrypted"
        } else {
            ""
        },
    );
    for e in &a.index().entries {
        let Some(name) = known
            .get(&e.id)
            .filter(|n| n.to_ascii_lowercase().ends_with(".mix"))
        else {
            continue;
        };
        match a.open_nested(*e) {
            Ok(n) => ok &= check_tree(&n, name, known, depth + 1, totals),
            Err(err) => {
                println!("FAIL {}  {name}: {err}", "  ".repeat(depth));
                ok = false;
            }
        }
    }
    ok
}
