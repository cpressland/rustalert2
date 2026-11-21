//! `parity`: compare what the original game and our engine hold during a
//! scenario. See `docs/spec/parity.md`.

// The command dispatch is long but flat.
#![allow(clippy::too_many_lines)]

use std::fs::File;
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ra2_parity::diff::compare;
use ra2_parity::log::{Event, Log, events};
use ra2_parity::scenario::Scenario;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Compare a reference log (the original game) with a candidate (our
    /// engine) under a scenario's checks. Without a candidate, show what the
    /// reference holds for each check.
    Diff {
        scenario: PathBuf,
        reference: PathBuf,
        candidate: Option<PathBuf>,
    },
    /// Play a scenario in our engine and write its log, for `diff`.
    Run {
        scenario: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
        /// The player's install, for the rules (default: found the usual way).
        #[arg(long)]
        game_dir: Option<PathBuf>,
        #[arg(long, default_value_t = 1)]
        seed: u32,
    },
    /// List what was created and removed during a log.
    Events {
        log: PathBuf,
        /// Only objects of this rules ID.
        #[arg(long = "type")]
        type_id: Option<String>,
    },
    /// Summarise a log: frames, houses, object counts by type.
    Summary { log: PathBuf },
    /// Write a scenario's generated map.
    Mkmap {
        scenario: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Record a scenario in the original game through the recomp's observer.
    Record {
        scenario: PathBuf,
        /// The recomp host built with the observer.
        #[arg(long)]
        recomp: PathBuf,
        /// Where the log goes.
        #[arg(short, long)]
        out: PathBuf,
        /// The player's install (default: found the usual way).
        #[arg(long)]
        game_dir: Option<PathBuf>,
        /// Working folder for the run's game folder and console log.
        #[arg(long, default_value = "target/parity")]
        work: PathBuf,
        /// The skirmish list position to start (9999: the last).
        #[arg(long, default_value_t = 9999)]
        scen_index: u32,
        /// The file the game loads for that position; the scenario map is
        /// written under this name, overriding the shipped one.
        #[arg(long, default_value = "XMP29U2.MAP")]
        map_as: String,
        #[arg(long, default_value_t = 600)]
        watchdog: u32,
        /// Extra files to add to the game folder (repeatable).
        #[arg(long)]
        extra: Vec<PathBuf>,
    },
}

fn load_scenario(p: &Path) -> io::Result<Scenario> {
    Scenario::parse(&std::fs::read_to_string(p)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}: {e}", p.display())))
}

fn read_log(p: &Path) -> io::Result<Log> {
    Log::read(BufReader::new(File::open(p)?))
        .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", p.display())))
}

fn run(cmd: Command) -> io::Result<bool> {
    match cmd {
        Command::Diff {
            scenario,
            reference,
            candidate,
        } => {
            let s = Scenario::parse(&std::fs::read_to_string(&scenario)?).map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}: {e}", scenario.display()),
                )
            })?;
            let reference = read_log(&reference)?;
            let candidate = candidate.as_deref().map(read_log).transpose()?;
            let out = compare(&s, &reference, candidate.as_ref());
            println!(
                "{}: {}",
                s.name,
                if candidate.is_some() {
                    "reference vs candidate"
                } else {
                    "reference only"
                }
            );
            for o in &out {
                println!("  {o}");
            }
            let failed = out.iter().filter(|o| !o.pass).count();
            println!("{} of {} checks pass", out.len() - failed, out.len());
            Ok(failed == 0)
        }
        Command::Events { log, type_id } => {
            let log = read_log(&log)?;
            for e in events(&log) {
                let (frame, verb, t, owner, extra) = match &e {
                    Event::Created {
                        frame,
                        type_id,
                        owner,
                        ..
                    } => (*frame, "created", type_id, *owner, String::new()),
                    Event::Removed {
                        frame,
                        type_id,
                        owner,
                        last_hp,
                        ..
                    } => (
                        *frame,
                        "removed",
                        type_id,
                        *owner,
                        format!(" (last hp {last_hp})"),
                    ),
                };
                if type_id.as_ref().is_none_or(|w| w.eq_ignore_ascii_case(t)) {
                    println!("{frame:>7} {verb:<8} {t:<10} house {owner}{extra}");
                }
            }
            Ok(true)
        }
        Command::Run {
            scenario,
            out,
            game_dir,
            seed,
        } => {
            use ra2_vfs::install::{Game, Inspection, SearchContext};
            let s = load_scenario(&scenario)?;
            let inspection = match game_dir {
                Some(d) => Inspection::of(&d)?,
                None => {
                    SearchContext::from_env()
                        .find(Game::YurisRevenge)
                        .map_err(io::Error::other)?
                        .inspection
                }
            };
            let mut vfs = ra2_vfs::mount::Vfs::new(inspection);
            vfs.mount_startup(Game::YurisRevenge);
            let read = |n: &str| -> io::Result<ra2_formats::ini::Ini> {
                let d = vfs.read(n)?.ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("{n}: not found"))
                })?;
                Ok(ra2_formats::ini::Ini::parse(&d))
            };
            let map = match &s.generate {
                Some(spec) => {
                    ra2_formats::ini::Ini::parse(ra2_parity::mapgen::write(spec).as_bytes())
                }
                None => read(&s.map)?,
            };
            // Map sections override the rules, as in the game.
            let mut rules_ini = read("rulesmd.ini")?;
            for section in map.sections() {
                for (k, v) in section.entries() {
                    rules_ini.put(&section.name, k, v);
                }
            }
            let rules = ra2_rules::Rules::from_ini(&rules_ini, &read("artmd.ini")?);
            let log = ra2_parity::engine::run(&s, &rules, &map, seed);
            log.write(io::BufWriter::new(File::create(&out)?))?;
            println!(
                "{}: {} frames written to {}",
                s.name,
                log.frames.len(),
                out.display()
            );
            Ok(true)
        }
        Command::Mkmap { scenario, out } => {
            let s = load_scenario(&scenario)?;
            let spec = s.generate.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "the scenario has no [generate] map",
                )
            })?;
            std::fs::write(&out, ra2_parity::mapgen::write(&spec))?;
            println!("wrote {}", out.display());
            Ok(true)
        }
        Command::Record {
            scenario,
            recomp,
            out,
            game_dir,
            work,
            scen_index,
            map_as,
            watchdog,
            extra,
        } => {
            let s = load_scenario(&scenario)?;
            let install = match game_dir {
                Some(d) => d,
                None => {
                    ra2_vfs::install::SearchContext::from_env()
                        .find(ra2_vfs::install::Game::YurisRevenge)
                        .map_err(io::Error::other)?
                        .inspection
                        .root
                }
            };
            let work = work.join(&s.name);
            std::fs::create_dir_all(&work)?;
            let r = ra2_parity::record::Recorder {
                exe: recomp,
                install,
                work: work.clone(),
                scen_index,
                map_as,
                watchdog,
                extra,
            };
            let (console, runs) = r.record_matching(&s, &std::path::absolute(&out)?, 8)?;
            let seed = console
                .lines()
                .filter_map(|l| l.strip_prefix("[game] Seed is "))
                .next_back()
                .unwrap_or("?")
                .to_string();
            let log = read_log(&out)?;
            if let (Some(a), Some(b)) = (log.frames.first(), log.frames.last()) {
                println!(
                    "{}: frames {}..{} ({} observed), seed {seed}, {runs} run(s), in {}",
                    s.name,
                    a.frame,
                    b.frame,
                    log.frames.len(),
                    out.display()
                );
            } else {
                println!(
                    "{}: nothing observed; the console is in {}",
                    s.name,
                    work.join("run.log").display()
                );
                for l in console
                    .lines()
                    .filter(|l| l.contains("messagebox") || l.contains("watchdog"))
                    .take(5)
                {
                    println!("  {l}");
                }
                return Ok(false);
            }
            Ok(true)
        }
        Command::Summary { log } => {
            let log = read_log(&log)?;
            let (Some(first), Some(last)) = (log.frames.first(), log.frames.last()) else {
                println!("empty log");
                return Ok(false);
            };
            println!(
                "frames {}..{} ({} observed)",
                first.frame,
                last.frame,
                log.frames.len()
            );
            for h in &last.houses {
                println!(
                    "  house {} {:<12} credits {:>6}  power {}/{}{}",
                    h.index,
                    h.country,
                    h.credits,
                    h.power_drain,
                    h.power_output,
                    if h.defeated { "  defeated" } else { "" }
                );
            }
            let mut counts = std::collections::BTreeMap::new();
            for o in &last.objects {
                *counts.entry((o.owner, o.type_id.clone())).or_insert(0) += 1;
            }
            for ((owner, t), n) in counts {
                println!("  house {owner} {t:<10} x{n}");
            }
            Ok(true)
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse().command) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
