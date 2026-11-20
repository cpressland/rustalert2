//! Recording a scenario in the original game, through the recomp and its
//! observer (`parity/recomp/`). See `docs/spec/parity.md`.
//!
//! A private game folder is laid out with hard links to the player's
//! install (the game writes into its folder), the scenario's map is written
//! into it, `RA2MD.INI` is set to start a one-AI skirmish on that map, and
//! the recomp runs headless, pressing Single Player, Skirmish and Start Game.
//! The observer logs every frame and ends the game at the scenario's end.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::mapgen;
use crate::scenario::Scenario;

/// Main menu, Single Player, Skirmish: dialog resource IDs and the buttons
/// pressed, from the recomp's dialog map (`work/dialogs.json`).
const MENU_PRESSES: [&str; 3] = ["0xE2:1667@1", "0x100:1401@2", "0x102:1559@3"];

/// How to record.
#[derive(Debug, Clone)]
pub struct Recorder {
    /// The recomp's host binary, built with the observer.
    pub exe: PathBuf,
    /// The player's install.
    pub install: PathBuf,
    /// Where to lay out the run's game folder (kept for inspection).
    pub work: PathBuf,
    /// The skirmish list position to start.
    pub scen_index: u32,
    /// The file name the game loads for that position. The scenario's map
    /// is written under this name: a loose file overrides the shipped map
    /// in the archives ([vfs.md]), so the menu needs no new entry.
    pub map_as: String,
    /// Seconds before the recomp's watchdog stops the run.
    pub watchdog: u32,
    /// Extra files to put in the game folder (experiments: a loose INI, an
    /// extra archive).
    pub extra: Vec<PathBuf>,
}

/// Set `key=value` in `[section]` of INI text, adding either if missing.
pub fn set_ini(text: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("[{section}]");
    let mut out = Vec::new();
    let (mut in_section, mut done, mut seen_section) = (false, false, false);
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            if in_section && !done {
                out.push(format!("{key}={value}"));
                done = true;
            }
            in_section = t.eq_ignore_ascii_case(&header);
            seen_section |= in_section;
        } else if in_section
            && !done
            && t.split('=')
                .next()
                .is_some_and(|k| k.trim().eq_ignore_ascii_case(key))
        {
            out.push(format!("{key}={value}"));
            done = true;
            continue;
        }
        out.push(line.to_string());
    }
    if in_section && !done {
        out.push(format!("{key}={value}"));
    } else if !seen_section {
        out.push(String::new());
        out.push(header);
        out.push(format!("{key}={value}"));
    }
    let mut s = out.join("\r\n");
    s.push_str("\r\n");
    s
}

/// Hard-link every file of `src` into `dst` (symlink where linking fails),
/// except `RA2MD.INI`, which is returned for editing.
fn farm(src: &Path, dst: &Path) -> io::Result<String> {
    let mut ini = String::new();
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let (from, to) = (entry.path(), dst.join(entry.file_name()));
        if entry.file_type()?.is_dir() {
            fs::create_dir_all(&to)?;
            farm(&from, &to)?;
        } else if entry
            .file_name()
            .to_string_lossy()
            .eq_ignore_ascii_case("RA2MD.INI")
        {
            ini = String::from_utf8_lossy(&fs::read(&from)?).into_owned();
        } else if !to.exists() && fs::hard_link(&from, &to).is_err() {
            std::os::unix::fs::symlink(&from, &to)?;
        }
    }
    Ok(ini)
}

impl Recorder {
    /// Record `scenario` to `out`, retrying (up to `attempts` times) until the
    /// human starts where the scenario expects. Returns the console output
    /// of the run kept, and how many runs it took.
    pub fn record_matching(
        &self,
        scenario: &Scenario,
        out: &Path,
        attempts: u32,
    ) -> io::Result<(String, u32)> {
        for n in 1..=attempts.max(1) {
            let console = self.record(scenario, out)?;
            let Some([sx, sy]) = scenario.human_start else {
                return Ok((console, n));
            };
            let log = crate::log::Log::read(io::BufReader::new(fs::File::open(out)?))?;
            let start = log
                .frames
                .first()
                .and_then(|f| f.objects.iter().find(|o| o.owner == 0));
            if start.is_some_and(|o| o.x / 256 == sx && o.y / 256 == sy) {
                return Ok((console, n));
            }
        }
        Err(io::Error::other(format!(
            "the human never started at the expected cell in {attempts} runs"
        )))
    }

    /// Record `scenario` to `out` once. Returns the recomp's console output.
    pub fn record(&self, scenario: &Scenario, out: &Path) -> io::Result<String> {
        let game = self.work.join("game");
        if game.exists() {
            fs::remove_dir_all(&game)?;
        }
        fs::create_dir_all(&game)?;
        let mut ini = farm(&self.install, &game)?;
        // The intro off, one hard AI in slot 1, the other slots closed,
        // the scenario map selected, no starting units, slowest speed
        // bound lifted by the headless host.
        for (k, v) in [
            ("ScenIndex", self.scen_index.to_string()),
            ("UnitCount", "0".into()),
            ("Slot01", "4,8,1".into()),
        ] {
            ini = set_ini(&ini, "Skirmish", k, &v);
        }
        for slot in 2..=7 {
            ini = set_ini(&ini, "Skirmish", &format!("Slot{slot:02}"), "1,-2,-2");
        }
        ini = set_ini(&ini, "Intro", "Play", "no");
        fs::write(game.join("RA2MD.INI"), ini)?;
        if let Some(spec) = &scenario.generate {
            fs::write(game.join(&self.map_as), mapgen::write(spec))?;
        }
        for f in &self.extra {
            let name = f.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "extra file has no name")
            })?;
            let to = game.join(name);
            if to.exists() {
                fs::remove_file(&to)?;
            }
            fs::copy(f, to)?;
        }
        let mut cmd = Command::new(&self.exe);
        cmd.args([
            "--headless",
            "--run",
            "--mute",
            "--debuglog",
            "--watchdog",
            &self.watchdog.to_string(),
        ])
        .arg("--exe")
        .arg(game.join("gamemd.exe"))
        .arg("--game")
        .arg(&game);
        for p in MENU_PRESSES {
            cmd.args(["--press", p]);
        }
        cmd.env("RA2_OBSERVE", out)
            .env("RA2_OBSERVE_END", scenario.end_frame.to_string())
            .current_dir(&self.work);
        let output = cmd.output()?;
        let console = String::from_utf8_lossy(&output.stdout).into_owned()
            + &String::from_utf8_lossy(&output.stderr);
        fs::write(self.work.join("run.log"), &console)?;
        Ok(console)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_editing() {
        let text = "[Intro]\r\nPlay=yes\r\n\r\n[Skirmish]\r\nScenIndex=84\r\nGameSpeed=1\r\n";
        let t = set_ini(text, "Skirmish", "ScenIndex", "3");
        assert!(t.contains("ScenIndex=3") && !t.contains("ScenIndex=84"));
        let t = set_ini(&t, "Skirmish", "Slot02", "1,-2,-2");
        assert!(t.contains("GameSpeed=1\r\nSlot02=1,-2,-2"));
        let t = set_ini(&t, "Intro", "Play", "no");
        assert!(t.contains("[Intro]\r\nPlay=no"));
        let t = set_ini(&t, "Video", "X", "1");
        assert!(t.ends_with("[Video]\r\nX=1\r\n"));
    }
}
