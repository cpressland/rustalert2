//! `ra2tool`: command-line tools for working with a game install.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
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
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Doctor { game_dir } => doctor(game_dir),
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
