use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Parser, Subcommand};

mod commands;
mod config;
mod error;
mod git;
mod utils;

use config::Config;

/// Entry point for the repo-sync CLI.
///
/// Defines two subcommands:
/// - `clone`: clone repositories listed in a file into an output directory
/// - `sync`: update existing repositories (fetch + fast-forward, never `git pull`)
///
/// Each subcommand requires:
/// - `--file` / `-f`: path to a text file containing one repo URL per line
/// - `--out` / `-o`: local directory where repositories are cloned/synced
///
/// `--verbose` / `-v` is a global flag, so it may appear before or after
/// the subcommand (e.g. `repo-sync -v sync ...` or `repo-sync sync ... -v`).
///
/// Example:
///   repo-sync clone -f repos.txt -o ./repos
///   repo-sync sync -f repos.txt -o ./repos
///
/// Exits with code 1 if the repo list can't be read, `git` isn't on
/// `PATH`, or any repository fails to clone or sync. Clap's own usage
/// errors (e.g. a missing required argument, or no arguments at all) exit
/// with code 2.
fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(e) = git::preflight() {
        eprintln!("error: git not found on PATH ({e})");
        return ExitCode::FAILURE;
    }

    match cli.command {
        Commands::Clone(args) => {
            let config = Config::new(args.file, args.out).with_verbose(cli.verbose);
            commands::clone::run(&config)
        }
        Commands::Sync(args) => {
            let config = Config::new(args.file, args.out).with_verbose(cli.verbose);
            commands::sync::run(&config)
        }
    }
}

/// Clone or sync multiple git repositories from a file
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Clone repositories from a file into an output directory
    Clone(RepoArgs),
    /// Sync existing repositories (fetch + branch fast-forward)
    Sync(RepoArgs),
}

#[derive(Args)]
struct RepoArgs {
    /// Repository list file
    #[arg(short, long)]
    file: PathBuf,

    /// Output directory
    #[arg(short, long)]
    out: PathBuf,
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::Cli;

    /// Catches derive-macro mistakes (conflicting short flags, invalid
    /// `#[command(...)]`/`#[arg(...)]` attributes, etc.) that would
    /// otherwise only surface the first time a user hit that exact path.
    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }
}
