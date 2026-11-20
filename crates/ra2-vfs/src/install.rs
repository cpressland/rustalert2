//! Finding and checking the player's game install.
//!
//! We never ship or download game data: the engine reads the files of the
//! player's own copy of *Red Alert 2* / *Yuri's Revenge*. This module finds
//! that copy and says whether it has what each game needs.
//!
//! The search order is:
//!
//! 1. `RA2_DATA`, if set.
//! 2. `game_dir` in `config.toml` in the config directory, if set.
//! 3. Known install locations: Steam libraries, the EA App and Origin, and on
//!    macOS the Windows Steam inside `CrossOver` and Whisky bottles.
//!
//! A path given by 1 or 2 is authoritative: if it is not a valid install we
//! report what is wrong with it rather than silently using another copy.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// The folder the game installs to under `steamapps/common`.
pub const STEAM_FOLDER: &str = "Command & Conquer Red Alert II";
/// The folder the EA App and Origin install to.
pub const EA_FOLDER: &str = "Command and Conquer Red Alert II";

/// The two games the engine runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Game {
    /// *Yuri's Revenge*, the expansion. It also needs the base game's files.
    YurisRevenge,
    /// *Red Alert 2*.
    RedAlert2,
}

impl Game {
    /// The files a game cannot start without, lower case.
    pub fn required_files(self) -> &'static [&'static str] {
        match self {
            Game::RedAlert2 => &["ra2.mix", "language.mix"],
            Game::YurisRevenge => &["ra2.mix", "language.mix", "ra2md.mix", "langmd.mix"],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Game::YurisRevenge => "Yuri's Revenge",
            Game::RedAlert2 => "Red Alert 2",
        }
    }
}

/// Where a candidate install came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// The `RA2_DATA` environment variable.
    Environment,
    /// `game_dir` in the config file at this path.
    ConfigFile(PathBuf),
    /// A path given on the command line.
    Given,
    /// A Steam library.
    Steam,
    /// The EA App's default folder.
    EaApp,
    /// Origin's default folder.
    Origin,
    /// Inside a `CrossOver` bottle, by name.
    CrossOver(String),
    /// Inside a Whisky bottle, by name.
    Whisky(String),
}

impl Origin {
    /// Whether the user chose this path, so it must not be skipped over.
    pub fn is_explicit(&self) -> bool {
        matches!(
            self,
            Origin::Environment | Origin::ConfigFile(_) | Origin::Given
        )
    }
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Origin::Environment => f.write_str("RA2_DATA"),
            Origin::ConfigFile(p) => write!(f, "game_dir in {}", p.display()),
            Origin::Given => f.write_str("given path"),
            Origin::Steam => f.write_str("Steam library"),
            Origin::EaApp => f.write_str("EA App"),
            Origin::Origin => f.write_str("Origin"),
            Origin::CrossOver(b) => write!(f, "CrossOver bottle \"{b}\""),
            Origin::Whisky(b) => write!(f, "Whisky bottle \"{b}\""),
        }
    }
}

/// A folder that might hold the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub path: PathBuf,
    pub origin: Origin,
}

/// Which store a copy came from, as far as we can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    Steam,
    EaApp,
    Unknown,
}

impl fmt::Display for Edition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Edition::Steam => "Steam",
            Edition::EaApp => "EA App / Origin",
            Edition::Unknown => "unknown",
        })
    }
}

/// What we found in a folder.
#[derive(Debug, Clone)]
pub struct Inspection {
    pub root: PathBuf,
    /// Every file in the folder (not recursive), lower-case name to real name.
    /// The game is case-insensitive; Linux file systems are not.
    pub files: BTreeMap<String, String>,
    pub edition: Edition,
    /// The executable versions, from their sizes, when they are present. We
    /// never run them; this only helps when reading bug reports.
    pub gamemd_version: Option<&'static str>,
    pub game_version: Option<&'static str>,
}

impl Inspection {
    /// Look at a folder. Fails only if it cannot be read as a directory.
    pub fn of(root: &Path) -> std::io::Result<Inspection> {
        let mut files = BTreeMap::new();
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                let name = entry.file_name().to_string_lossy().into_owned();
                files.insert(name.to_lowercase(), name);
            }
        }
        let size = |name: &str| {
            files
                .get(name)
                .and_then(|real| fs::metadata(root.join(real)).ok())
                .map(|m| m.len())
        };
        let gamemd_version = size("gamemd.exe").and_then(|s| match s {
            // Steam's signature, then EA's (re-signed, 296 bytes longer).
            5_286_208 | 5_286_504 => Some("1.001"),
            _ => None,
        });
        let game_version = size("game.exe").and_then(|s| match s {
            5_077_312 | 5_077_608 => Some("1.006"),
            _ => None,
        });
        let edition = if root.join("Core").join("Activation.dll").is_file() {
            Edition::EaApp
        } else if root
            .components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case("steamapps"))
        {
            Edition::Steam
        } else {
            Edition::Unknown
        };
        Ok(Inspection {
            root: root.to_path_buf(),
            files,
            edition,
            gamemd_version,
            game_version,
        })
    }

    /// The real path of a file, looked up case-insensitively.
    pub fn file(&self, name: &str) -> Option<PathBuf> {
        self.files
            .get(&name.to_lowercase())
            .map(|real| self.root.join(real))
    }

    /// The required files of `game` that are missing.
    pub fn missing(&self, game: Game) -> Vec<&'static str> {
        game.required_files()
            .iter()
            .copied()
            .filter(|f| !self.files.contains_key(*f))
            .collect()
    }

    pub fn has(&self, game: Game) -> bool {
        self.missing(game).is_empty()
    }
}

/// A valid install and where it came from.
#[derive(Debug, Clone)]
pub struct Install {
    pub origin: Origin,
    pub inspection: Inspection,
}

/// Why a candidate was not used.
#[derive(Debug, Clone)]
pub enum Rejection {
    /// The folder does not exist or cannot be read.
    Unreadable(String),
    /// The folder is there but lacks these files.
    Missing(Vec<&'static str>),
}

/// The search failed.
#[derive(Debug, Clone)]
pub struct NotFound {
    pub game: Game,
    /// Every place we looked, in order, and why it was no good.
    pub tried: Vec<(Candidate, Rejection)>,
}

impl fmt::Display for NotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "no {} install found. Looked in:", self.game.name())?;
        if self.tried.is_empty() {
            writeln!(
                f,
                "  (nowhere: set RA2_DATA to the folder holding ra2md.mix)"
            )?;
        }
        for (c, why) in &self.tried {
            match why {
                Rejection::Unreadable(e) => {
                    writeln!(f, "  {} ({}): {e}", c.path.display(), c.origin)?;
                }
                Rejection::Missing(m) => {
                    writeln!(
                        f,
                        "  {} ({}): missing {}",
                        c.path.display(),
                        c.origin,
                        m.join(", ")
                    )?;
                }
            }
        }
        Ok(())
    }
}

impl std::error::Error for NotFound {}

/// The parts of the environment the search reads, so tests can supply their own.
#[derive(Debug, Clone, Default)]
pub struct SearchContext {
    pub home: Option<PathBuf>,
    pub ra2_data: Option<PathBuf>,
    pub config_dir: Option<PathBuf>,
    /// `%APPDATA%`, used for the config directory on Windows.
    pub appdata: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
}

impl SearchContext {
    /// Read the real environment.
    pub fn from_env() -> SearchContext {
        let var = |k: &str| {
            std::env::var_os(k)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        let mut ctx = SearchContext {
            home: var("HOME").or_else(|| var("USERPROFILE")),
            ra2_data: var("RA2_DATA"),
            config_dir: None,
            appdata: var("APPDATA"),
            xdg_config_home: var("XDG_CONFIG_HOME"),
        };
        ctx.config_dir = ctx.default_config_dir();
        ctx
    }

    /// Where config, saves and logs live on this platform.
    pub fn default_config_dir(&self) -> Option<PathBuf> {
        if cfg!(target_os = "macos") {
            self.home
                .as_ref()
                .map(|h| h.join("Library/Application Support/rustalert2"))
        } else if cfg!(windows) {
            self.appdata.as_ref().map(|a| a.join("rustalert2"))
        } else {
            self.xdg_config_home
                .clone()
                .or_else(|| self.home.as_ref().map(|h| h.join(".config")))
                .map(|c| c.join("rustalert2"))
        }
    }

    /// The config file's path, whether or not it exists.
    pub fn config_file(&self) -> Option<PathBuf> {
        self.config_dir.as_ref().map(|d| d.join("config.toml"))
    }

    /// Every candidate, in search order. Explicit choices come first.
    pub fn candidates(&self) -> Vec<Candidate> {
        let mut out = Vec::new();
        if let Some(p) = &self.ra2_data {
            out.push(Candidate {
                path: p.clone(),
                origin: Origin::Environment,
            });
        }
        if let Some(cfg) = self.config_file()
            && let Some(dir) = read_config_game_dir(&cfg)
        {
            out.push(Candidate {
                path: dir,
                origin: Origin::ConfigFile(cfg),
            });
        }
        if let Some(home) = &self.home {
            out.extend(discovered(home));
        }
        if cfg!(windows) {
            out.extend(windows_drive_candidates(Path::new("C:\\"), &|o| o));
        }
        dedup(out)
    }

    /// Find an install of `game`.
    pub fn find(&self, game: Game) -> Result<Install, NotFound> {
        find_in(self.candidates(), game)
    }
}

/// Check candidates in order and return the first that has `game`.
/// An explicit candidate that fails stops the search.
pub fn find_in(candidates: Vec<Candidate>, game: Game) -> Result<Install, NotFound> {
    let mut tried = Vec::new();
    for c in candidates {
        let explicit = c.origin.is_explicit();
        match Inspection::of(&c.path) {
            Ok(i) if i.has(game) => {
                return Ok(Install {
                    origin: c.origin,
                    inspection: i,
                });
            }
            Ok(i) => {
                let missing = i.missing(game);
                tried.push((c, Rejection::Missing(missing)));
            }
            Err(e) => {
                // Unreadable discovered paths are just places nothing is installed.
                if !explicit {
                    continue;
                }
                tried.push((c, Rejection::Unreadable(e.to_string())));
            }
        }
        if explicit {
            break;
        }
    }
    Err(NotFound { game, tried })
}

#[derive(serde::Deserialize)]
struct ConfigFile {
    game_dir: Option<PathBuf>,
}

fn read_config_game_dir(path: &Path) -> Option<PathBuf> {
    let text = fs::read_to_string(path).ok()?;
    toml::from_str::<ConfigFile>(&text).ok()?.game_dir
}

fn dedup(cands: Vec<Candidate>) -> Vec<Candidate> {
    let mut seen = std::collections::BTreeSet::new();
    cands
        .into_iter()
        .filter(|c| seen.insert(c.path.clone()))
        .collect()
}

type BottleOrigin = fn(String) -> Origin;

/// Known locations under a home directory, on every platform. A missing
/// folder costs only a failed `read_dir`, so we look in all of them.
fn discovered(home: &Path) -> Vec<Candidate> {
    let mut out = Vec::new();

    // Native Steam installs (Linux, including Flatpak Steam; macOS).
    for steam in [
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
        home.join("Library/Application Support/Steam"),
    ] {
        for lib in steam_libraries(&steam, None) {
            out.push(Candidate {
                path: lib.join("steamapps/common").join(STEAM_FOLDER),
                origin: Origin::Steam,
            });
        }
    }

    // Windows installs inside Wine bottles on macOS.
    let bottles: [(PathBuf, BottleOrigin); 2] = [
        (
            home.join("Library/Application Support/CrossOver/Bottles"),
            Origin::CrossOver,
        ),
        (
            home.join("Library/Containers/com.isaacmarovitz.Whisky/Bottles"),
            Origin::Whisky,
        ),
    ];
    for (dir, make) in bottles {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        let mut names: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        names.sort();
        for bottle in names {
            let name = bottle
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let wrap = |_: Origin| make(name.clone());
            out.extend(windows_drive_candidates(&bottle.join("drive_c"), &wrap));
        }
    }
    out
}

/// Candidates inside a Windows `C:` drive, real or a Wine bottle's `drive_c`.
/// `wrap` turns the store's origin into the bottle's, when in a bottle.
fn windows_drive_candidates(drive_c: &Path, wrap: &dyn Fn(Origin) -> Origin) -> Vec<Candidate> {
    let mut out = Vec::new();
    let bottle_root = drive_c.parent().filter(|_| drive_c.ends_with("drive_c"));
    for pf in ["Program Files (x86)", "Program Files"] {
        let steam = drive_c.join(pf).join("Steam");
        for lib in steam_libraries(&steam, bottle_root) {
            out.push(Candidate {
                path: lib.join("steamapps/common").join(STEAM_FOLDER),
                origin: wrap(Origin::Steam),
            });
        }
    }
    out.push(Candidate {
        path: drive_c.join("Program Files/EA Games").join(EA_FOLDER),
        origin: wrap(Origin::EaApp),
    });
    out.push(Candidate {
        path: drive_c
            .join("Program Files (x86)/Origin Games")
            .join(EA_FOLDER),
        origin: wrap(Origin::Origin),
    });
    out
}

/// A Steam root and every extra library listed in its `libraryfolders.vdf`.
/// Inside a bottle, the VDF holds Windows paths (`D:\\Games`), which are
/// mapped to the bottle's `drive_d`.
fn steam_libraries(steam_root: &Path, bottle: Option<&Path>) -> Vec<PathBuf> {
    let mut libs = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps/libraryfolders.vdf");
    if let Ok(text) = fs::read_to_string(vdf) {
        for path in vdf_paths(&text) {
            let mapped = match bottle {
                Some(b) => windows_path_in_bottle(&path, b),
                None => Some(PathBuf::from(path)),
            };
            libs.extend(mapped);
        }
    }
    libs
}

/// The values of `"path"` keys in a Steam VDF file, unescaped.
pub(crate) fn vdf_paths(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("\"path\"")?.trim();
            let inner = rest.strip_prefix('"')?.strip_suffix('"')?;
            Some(inner.replace("\\\\", "\\"))
        })
        .collect()
}

/// `C:\Program Files\Steam` → `<bottle>/drive_c/Program Files/Steam`.
pub(crate) fn windows_path_in_bottle(path: &str, bottle: &Path) -> Option<PathBuf> {
    let mut chars = path.chars();
    let drive = chars.next()?.to_ascii_lowercase();
    if !drive.is_ascii_alphabetic() || chars.next()? != ':' {
        return None;
    }
    let mut out = bottle.join(format!("drive_{drive}"));
    for part in chars.as_str().split(['\\', '/']).filter(|p| !p.is_empty()) {
        out.push(part);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, names: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        for n in names {
            fs::write(dir.join(n), b"").unwrap();
        }
    }

    const YR: &[&str] = &["RA2.MIX", "language.mix", "ra2md.mix", "LANGMD.MIX"];

    #[test]
    fn case_insensitive_and_missing() {
        let t = tempfile::tempdir().unwrap();
        touch(t.path(), &["RA2.MIX", "Language.mix"]);
        let i = Inspection::of(t.path()).unwrap();
        assert!(i.has(Game::RedAlert2));
        assert_eq!(
            i.missing(Game::YurisRevenge),
            vec!["ra2md.mix", "langmd.mix"]
        );
        assert_eq!(i.file("ra2.mix").unwrap(), t.path().join("RA2.MIX"));
    }

    #[test]
    fn explicit_path_is_authoritative() {
        let t = tempfile::tempdir().unwrap();
        let bad = t.path().join("bad");
        let good = t
            .path()
            .join("home/.local/share/Steam/steamapps/common")
            .join(STEAM_FOLDER);
        touch(&bad, &["ra2.mix"]);
        touch(&good, YR);
        let ctx = SearchContext {
            home: Some(t.path().join("home")),
            ra2_data: Some(bad),
            ..Default::default()
        };
        let err = ctx.find(Game::YurisRevenge).unwrap_err();
        assert_eq!(err.tried.len(), 1);
        assert!(matches!(&err.tried[0].1, Rejection::Missing(m) if m.contains(&"ra2md.mix")));

        let ctx = SearchContext {
            ra2_data: None,
            ..ctx
        };
        let found = ctx.find(Game::YurisRevenge).unwrap();
        assert_eq!(found.origin, Origin::Steam);
        assert_eq!(found.inspection.edition, Edition::Steam);
    }

    #[test]
    fn config_file_game_dir() {
        let t = tempfile::tempdir().unwrap();
        let game = t.path().join("my game");
        touch(&game, YR);
        let cfg = t.path().join("cfg");
        fs::create_dir_all(&cfg).unwrap();
        fs::write(
            cfg.join("config.toml"),
            format!("game_dir = {:?}\n", game.to_str().unwrap()),
        )
        .unwrap();
        let ctx = SearchContext {
            config_dir: Some(cfg.clone()),
            ..Default::default()
        };
        let found = ctx.find(Game::YurisRevenge).unwrap();
        assert_eq!(found.origin, Origin::ConfigFile(cfg.join("config.toml")));
    }

    #[test]
    fn crossover_bottle_with_extra_library() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path();
        let bottle = home.join("Library/Application Support/CrossOver/Bottles/Steam");
        let steam = bottle.join("drive_c/Program Files (x86)/Steam/steamapps");
        fs::create_dir_all(&steam).unwrap();
        fs::write(
            steam.join("libraryfolders.vdf"),
            "\"libraryfolders\"\n{\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\Games\\\\Steam\"\n\t}\n}\n",
        )
        .unwrap();
        let game = bottle
            .join("drive_d/Games/Steam/steamapps/common")
            .join(STEAM_FOLDER);
        touch(&game, YR);
        touch(&game.join("Core"), &["Activation.dll"]);
        let ctx = SearchContext {
            home: Some(home.to_path_buf()),
            ..Default::default()
        };
        let found = ctx.find(Game::YurisRevenge).unwrap();
        assert_eq!(found.origin, Origin::CrossOver("Steam".into()));
        assert_eq!(found.inspection.root, game);
        assert_eq!(found.inspection.edition, Edition::EaApp);
    }

    #[test]
    fn nothing_found_lists_only_real_folders() {
        let t = tempfile::tempdir().unwrap();
        let ctx = SearchContext {
            home: Some(t.path().to_path_buf()),
            ..Default::default()
        };
        let err = ctx.find(Game::YurisRevenge).unwrap_err();
        assert!(err.tried.is_empty());
        assert!(err.to_string().contains("RA2_DATA"));
    }

    #[test]
    fn windows_paths() {
        let b = Path::new("/b");
        assert_eq!(
            windows_path_in_bottle("D:\\Games\\Steam", b),
            Some(PathBuf::from("/b/drive_d/Games/Steam"))
        );
        assert_eq!(windows_path_in_bottle("/home/x", b), None);
        assert_eq!(
            vdf_paths("  \"path\"  \"C:\\\\Steam\"\n\"label\" \"\""),
            vec!["C:\\Steam".to_string()]
        );
    }
}
