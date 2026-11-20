//! `ra2tool`: command-line tools for working with a game install.

mod mix;
mod vfs;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use ra2_vfs::install::{Candidate, Game, Inspection, Origin, SearchContext, find_in};

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Find the game install and check it has what each game needs.
    Doctor {
        /// Check this folder instead of searching.
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Work with MIX archives. Archive paths are a file on disk or in the
    /// game install, then any nested archives: `ra2md.mix/localmd.mix`.
    #[command(subcommand)]
    Mix(MixCommand),
    /// The game's file system, mounted the way the game mounts it at startup.
    #[command(subcommand)]
    Vfs(VfsCommand),
}

#[derive(Clone, Copy, ValueEnum)]
enum GameArg {
    Yr,
    Ra2,
}

impl From<GameArg> for Game {
    fn from(g: GameArg) -> Game {
        match g {
            GameArg::Yr => Game::YurisRevenge,
            GameArg::Ra2 => Game::RedAlert2,
        }
    }
}

#[derive(Subcommand)]
enum VfsCommand {
    /// Mount the startup archives in order and show what each step did.
    Mounts {
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
    /// Show where files resolve: a loose file, or which mounted archive.
    Which {
        names: Vec<String>,
        #[arg(long, value_enum, default_value = "yr")]
        game: GameArg,
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum MixCommand {
    /// List an archive's files.
    Ls {
        archive: String,
        /// A file of extra names to recognise, one per line.
        #[arg(long)]
        names: Option<PathBuf>,
    },
    /// Extract files by name, to a folder or to standard output.
    X {
        archive: String,
        files: Vec<String>,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Open every archive in the install, and those nested in them, and
    /// check their indexes and checksums.
    Check {
        #[arg(long)]
        game_dir: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Doctor { game_dir } => doctor(game_dir),
        Command::Vfs(cmd) => exit(match cmd {
            VfsCommand::Mounts { game, game_dir } => vfs::mounts(game.into(), game_dir),
            VfsCommand::Which {
                names,
                game,
                game_dir,
            } => vfs::which(game.into(), game_dir, &names),
        }),
        Command::Mix(cmd) => {
            let result = match cmd {
                MixCommand::Ls { archive, names } => {
                    mix::ls(&archive, names.as_deref()).map(|()| true)
                }
                MixCommand::X {
                    archive,
                    files,
                    out,
                } => mix::extract(&archive, &files, out.as_deref()).map(|()| true),
                MixCommand::Check { game_dir } => mix::check(game_dir),
            };
            match result {
                Ok(true) => ExitCode::SUCCESS,
                Ok(false) => ExitCode::FAILURE,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn exit(result: std::io::Result<bool>) -> ExitCode {
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn doctor(game_dir: Option<PathBuf>) -> ExitCode {
    let ctx = SearchContext::from_env();
    let candidates = match game_dir {
        Some(path) => vec![Candidate {
            path,
            origin: Origin::Given,
        }],
        None => ctx.candidates(),
    };
    if let Some(cfg) = ctx.config_file() {
        println!("config file: {}", cfg.display());
    }
    let install = match find_in(candidates, Game::YurisRevenge) {
        Ok(i) => i,
        Err(e) => {
            eprint!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let i: &Inspection = &install.inspection;
    println!("found:       {} ({})", i.root.display(), install.origin);
    println!("edition:     {}", i.edition);
    println!(
        "gamemd.exe:  {}",
        i.gamemd_version.unwrap_or("absent or unknown version")
    );
    println!(
        "game.exe:    {}",
        i.game_version.unwrap_or("absent or unknown version")
    );
    for game in [Game::YurisRevenge, Game::RedAlert2] {
        let missing = i.missing(game);
        if missing.is_empty() {
            println!("{:<16} ok", format!("{}:", game.name()));
        } else {
            println!(
                "{:<16} missing {}",
                format!("{}:", game.name()),
                missing.join(", ")
            );
        }
    }
    ExitCode::SUCCESS
}
