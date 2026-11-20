//! The game's file system: loose files and mounted archives, searched in the
//! game's order. See `docs/spec/vfs.md`.
//!
//! A name resolves to the first of:
//!
//! 1. a loose file in a mod overlay folder, highest priority first;
//! 2. a loose file in the game folder;
//! 3. the first mounted archive, in mount order, that holds it.
//!
//! Mounting an archive by name uses the same lookup, which is how archives
//! nested in other archives (`localmd.mix` in `ra2md.mix`) are found.

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ra2_formats::mix::Entry;

use crate::archive::{Archive, FileSource, Source, Window, read_all};
use crate::install::{Game, Inspection};

/// One mounted archive.
#[derive(Debug, Clone)]
pub struct Mount {
    /// The name it was mounted under, lower case.
    pub name: String,
    pub archive: Archive,
    /// Where its bytes came from.
    pub from: Location,
}

/// Where a file was found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// A loose file on disk.
    Loose(PathBuf),
    /// Inside a mounted archive: its position in mount order, and its name.
    Archive {
        mount: usize,
        archive: String,
        entry: Entry,
    },
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Location::Loose(p) => write!(f, "{}", p.display()),
            Location::Archive {
                mount,
                archive,
                entry,
            } => {
                write!(
                    f,
                    "{archive} (mount #{mount}, {} bytes at {})",
                    entry.size, entry.offset
                )
            }
        }
    }
}

/// The outcome of one step of a mount sequence.
#[derive(Debug)]
pub enum Mounted {
    /// Mounted, found at this location.
    Yes { name: String, from: Location },
    /// The game would skip it too: it isn't there.
    Absent { name: String },
    /// Already mounted under this name; the game would mount it twice, but
    /// the second copy can never win a lookup.
    Duplicate { name: String },
    /// It is there but couldn't be opened as an archive.
    Failed { name: String, error: io::Error },
}

/// One step of the game's startup mount sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Mount this archive if it can be found.
    One(&'static str),
    /// Mount the first of these that can be found (`THEMEMD.MIX`, else `THEME.MIX`).
    FirstOf(&'static [&'static str]),
    /// `NAME%02d.MIX` from 99 down to 0.
    NumberedDown(&'static str),
    /// Every loose file in the game folder matching `PREFIX*.MIX`, sorted.
    Wildcard(&'static str),
}

/// The archives mounted at startup, in order, for a game. Archives tied to a
/// scenario (theaters, sides) are mounted when one loads.
pub fn startup_sequence(game: Game) -> &'static [Step] {
    use Step::{FirstOf, NumberedDown, One, Wildcard};
    match game {
        Game::YurisRevenge => &[
            // WinMain [recomp 0x006BD7E8]
            One("langmd.mix"),
            One("language.mix"),
            // Bootstrap [recomp 0x005301A0]
            NumberedDown("expandmd"),
            One("ra2md.mix"),
            One("ra2.mix"),
            One("cachemd.mix"),
            One("cache.mix"),
            One("localmd.mix"),
            One("local.mix"),
            // Audio [recomp 0x00406B10]
            One("audiomd.mix"),
            One("audio.mix"),
            // Expansion caches [recomp 0x00530000]
            Wildcard("ecache"),
            Wildcard("elocal"),
            // Secondary [recomp 0x00530460]
            One("conqmd.mix"),
            One("genermd.mix"),
            One("generic.mix"),
            One("isogenmd.mix"),
            One("isogen.mix"),
            One("conquer.mix"),
            One("cameomd.mix"),
            One("cameo.mix"),
            Wildcard("mapsmd"),
            Wildcard("maps"),
            One("multimd.mix"),
            FirstOf(&["thememd.mix", "theme.mix"]),
            Wildcard("movmd"),
            Wildcard("movies"),
        ],
        // Red Alert 2: the same sequence without the YR archives. Not yet
        // checked against game.exe [hypothesis]; see the spec.
        Game::RedAlert2 => &[
            One("language.mix"),
            NumberedDown("expand"),
            One("ra2.mix"),
            One("cache.mix"),
            One("local.mix"),
            One("audio.mix"),
            Wildcard("ecache"),
            Wildcard("elocal"),
            One("generic.mix"),
            One("isogen.mix"),
            One("conquer.mix"),
            One("cameo.mix"),
            Wildcard("maps"),
            One("multi.mix"),
            One("theme.mix"),
            Wildcard("movies"),
        ],
    }
}

/// The file system.
#[derive(Debug)]
pub struct Vfs {
    game_dir: Inspection,
    /// Mod folders, highest priority first, each with its files by lower-case name.
    overlays: Vec<(PathBuf, BTreeMap<String, String>)>,
    mounts: Vec<Mount>,
}

impl Vfs {
    /// An empty file system over a game folder: loose files only, no archives.
    pub fn new(game_dir: Inspection) -> Vfs {
        Vfs {
            game_dir,
            overlays: Vec::new(),
            mounts: Vec::new(),
        }
    }

    /// Lay a mod folder over everything mounted so far and everything after:
    /// its loose files win over the game's. Later overlays win over earlier.
    pub fn add_overlay(&mut self, dir: &Path) -> io::Result<()> {
        let files = Inspection::of(dir)?.files;
        self.overlays.insert(0, (dir.to_path_buf(), files));
        Ok(())
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }

    /// Where a file would be read from.
    pub fn locate(&self, name: &str) -> Option<Location> {
        let lower = name.to_lowercase();
        for (dir, files) in &self.overlays {
            if let Some(real) = files.get(&lower) {
                return Some(Location::Loose(dir.join(real)));
            }
        }
        if let Some(path) = self.game_dir.file(&lower) {
            return Some(Location::Loose(path));
        }
        self.mounts.iter().enumerate().find_map(|(i, m)| {
            m.archive.find(&lower).map(|entry| Location::Archive {
                mount: i,
                archive: m.name.clone(),
                entry,
            })
        })
    }

    /// A file's bytes, read on demand.
    pub fn open(&self, name: &str) -> io::Result<Option<Arc<dyn Source>>> {
        Ok(match self.locate(name) {
            None => None,
            Some(Location::Loose(path)) => Some(Arc::new(FileSource::open(&path)?)),
            Some(Location::Archive { mount, entry, .. }) => {
                let window: Window = self.mounts[mount].archive.entry_source(entry)?;
                Some(Arc::new(window))
            }
        })
    }

    /// A file's bytes, read into memory.
    pub fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        self.open(name)?.map(|s| read_all(&*s)).transpose()
    }

    /// Mount an archive by name, found the way any file is found.
    pub fn mount(&mut self, name: &str) -> Mounted {
        let lower = name.to_lowercase();
        if self.mounts.iter().any(|m| m.name == lower) {
            return Mounted::Duplicate { name: lower };
        }
        let Some(from) = self.locate(&lower) else {
            return Mounted::Absent { name: lower };
        };
        let opened = match self.open(&lower) {
            Ok(Some(src)) => Archive::from_source(src),
            Ok(None) => return Mounted::Absent { name: lower },
            Err(e) => Err(e),
        };
        match opened {
            Ok(archive) => {
                self.mounts.push(Mount {
                    name: lower.clone(),
                    archive,
                    from: from.clone(),
                });
                Mounted::Yes { name: lower, from }
            }
            Err(error) => Mounted::Failed { name: lower, error },
        }
    }

    /// Run a mount sequence, returning what each name did.
    pub fn mount_sequence(&mut self, steps: &[Step]) -> Vec<Mounted> {
        let mut out = Vec::new();
        for step in steps {
            match *step {
                Step::One(name) => out.push(self.mount(name)),
                Step::FirstOf(names) => {
                    let pick = names
                        .iter()
                        .find(|n| self.locate(n).is_some())
                        .unwrap_or(&names[0]);
                    out.push(self.mount(pick));
                }
                Step::NumberedDown(prefix) => {
                    for n in (0..100).rev() {
                        let name = format!("{prefix}{n:02}.mix");
                        if self.locate(&name).is_some() {
                            out.push(self.mount(&name));
                        }
                    }
                }
                Step::Wildcard(prefix) => {
                    for name in self.loose_matching(prefix) {
                        out.push(self.mount(&name));
                    }
                }
            }
        }
        out
    }

    /// Mount everything the game mounts at startup.
    pub fn mount_startup(&mut self, game: Game) -> Vec<Mounted> {
        self.mount_sequence(startup_sequence(game))
    }

    /// Loose `PREFIX*.MIX` files in the overlays and the game folder, lower
    /// case and sorted. The game uses `FindFirstFile`, whose order on NTFS is
    /// by name.
    fn loose_matching(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.to_lowercase();
        let mut names: Vec<String> = self
            .overlays
            .iter()
            .flat_map(|(_, f)| f.keys())
            .chain(self.game_dir.files.keys())
            .filter(|n| {
                n.starts_with(&prefix)
                    && Path::new(n)
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("mix"))
            })
            .cloned()
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ra2_formats::mix::mix_id;
    use std::fs;

    fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut body = Vec::new();
        let mut entries = Vec::new();
        for (name, data) in files {
            entries.push((
                mix_id(name),
                u32::try_from(body.len()).unwrap(),
                u32::try_from(data.len()).unwrap(),
            ));
            body.extend_from_slice(data);
        }
        let mut out = 0u32.to_le_bytes().to_vec();
        out.extend_from_slice(&u16::try_from(files.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        for (id, off, size) in entries {
            for v in [id, off, size] {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&body);
        out
    }

    fn game_dir(files: &[(&str, Vec<u8>)]) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        for (name, data) in files {
            fs::write(t.path().join(name), data).unwrap();
        }
        t
    }

    #[test]
    fn first_mounted_wins_and_nesting_resolves() {
        let local = build(&[("rulesmd.ini", b"local")]);
        let ra2md = build(&[("localmd.mix", &local)]);
        let expand = build(&[("rulesmd.ini", b"expand")]);
        let t = game_dir(&[("RA2MD.MIX", ra2md), ("expandmd01.mix", expand)]);
        let mut vfs = Vfs::new(Inspection::of(t.path()).unwrap());
        let results = vfs.mount_startup(Game::YurisRevenge);
        let mounted: Vec<_> = results
            .iter()
            .filter_map(|r| {
                if let Mounted::Yes { name, .. } = r {
                    Some(name.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(mounted, ["expandmd01.mix", "ra2md.mix", "localmd.mix"]);
        // The expansion was mounted first, so it wins.
        assert_eq!(vfs.read("RULESMD.INI").unwrap().unwrap(), b"expand");
        match vfs.locate("localmd.mix").unwrap() {
            Location::Archive { archive, .. } => assert_eq!(archive, "ra2md.mix"),
            other @ Location::Loose(_) => panic!("{other:?}"),
        }
    }

    #[test]
    fn loose_files_and_overlays_win() {
        let local = build(&[("rulesmd.ini", b"archive")]);
        let t = game_dir(&[("localmd.mix", local), ("RulesMD.ini", b"loose".to_vec())]);
        let mut vfs = Vfs::new(Inspection::of(t.path()).unwrap());
        vfs.mount("localmd.mix");
        assert_eq!(vfs.read("rulesmd.ini").unwrap().unwrap(), b"loose");
        let m = tempfile::tempdir().unwrap();
        fs::write(m.path().join("rulesmd.ini"), b"mod").unwrap();
        vfs.add_overlay(m.path()).unwrap();
        assert_eq!(vfs.read("rulesmd.ini").unwrap().unwrap(), b"mod");
        assert!(vfs.read("artmd.ini").unwrap().is_none());
    }

    #[test]
    fn expansions_count_down_and_themes_pick_one() {
        let a = build(&[("x.ini", b"01")]);
        let b = build(&[("x.ini", b"02")]);
        let t = game_dir(&[
            ("expandmd01.mix", a),
            ("expandmd02.mix", b),
            ("thememd.mix", build(&[])),
            ("theme.mix", build(&[])),
        ]);
        let mut vfs = Vfs::new(Inspection::of(t.path()).unwrap());
        vfs.mount_startup(Game::YurisRevenge);
        assert_eq!(vfs.read("x.ini").unwrap().unwrap(), b"02");
        let names: Vec<_> = vfs.mounts().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["expandmd02.mix", "expandmd01.mix", "thememd.mix"]);
    }

    #[test]
    fn wildcards_and_duplicates() {
        let t = game_dir(&[
            ("MAPS02.MIX", build(&[])),
            ("maps01.mix", build(&[])),
            ("mapsmd03.mix", build(&[])),
            ("maps.txt", vec![]),
        ]);
        let mut vfs = Vfs::new(Inspection::of(t.path()).unwrap());
        let r = vfs.mount_sequence(&[Step::Wildcard("mapsmd"), Step::Wildcard("maps")]);
        let names: Vec<_> = vfs.mounts().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, ["mapsmd03.mix", "maps01.mix", "maps02.mix"]);
        assert!(
            r.iter()
                .any(|m| matches!(m, Mounted::Duplicate { name } if name == "mapsmd03.mix"))
        );
    }

    #[test]
    fn a_broken_archive_is_reported() {
        let t = game_dir(&[("ra2md.mix", b"\x00\x00\x00\x00\xff\xff".to_vec())]);
        let mut vfs = Vfs::new(Inspection::of(t.path()).unwrap());
        assert!(matches!(vfs.mount("ra2md.mix"), Mounted::Failed { .. }));
        assert!(matches!(vfs.mount("nope.mix"), Mounted::Absent { .. }));
    }
}
