use std::num::NonZeroUsize;
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
/// `--verbose` / `-v` and `--jobs` / `-j` are global flags, so either may
/// appear before or after the subcommand (e.g. `repo-sync -v sync ...` or
/// `repo-sync sync ... -v`). `--jobs` bounds how many repositories are
/// processed in parallel; when it's omitted, rayon picks its own default
/// (the number of CPUs, or `RAYON_NUM_THREADS` if set).
///
/// Example:
///   repo-sync clone -f repos.txt -o ./repos
///   repo-sync sync -f repos.txt -o ./repos
///
/// Exits with code 1 if the repo list can't be read, `git` isn't on
/// `PATH`, the thread pool for `-j`/`--jobs` fails to build, or any
/// repository fails to clone or sync. Clap's own usage errors (e.g. a
/// missing required argument, or no arguments at all) exit with code 2.
fn main() -> ExitCode {
    let cli = Cli::parse();

    if let Err(e) = git::preflight() {
        eprintln!("error: git not found on PATH ({e})");
        return ExitCode::FAILURE;
    }

    match cli.command {
        Commands::Clone(args) => {
            let config = Config::new(args.file, args.out)
                .with_verbose(cli.verbose)
                .with_jobs(cli.jobs);
            commands::clone::run(&config)
        }
        Commands::Sync(args) => {
            let config = Config::new(args.file, args.out)
                .with_verbose(cli.verbose)
                .with_jobs(cli.jobs);
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

    /// Number of repositories to process in parallel [default: number of CPUs]
    #[arg(short = 'j', long = "jobs", global = true)]
    jobs: Option<NonZeroUsize>,

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
    use std::num::NonZeroUsize;

    use clap::{CommandFactory, Parser};

    use super::Cli;

    /// Catches derive-macro mistakes (conflicting short flags, invalid
    /// `#[command(...)]`/`#[arg(...)]` attributes, etc.) that would
    /// otherwise only surface the first time a user hit that exact path.
    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    /// `-j 0` must be rejected at parse time (before any repository is
    /// touched), not accepted and then misbehave or panic later when
    /// handed to `rayon::ThreadPoolBuilder::num_threads`.
    #[test]
    fn jobs_zero_is_rejected_at_parse_time() {
        let result = Cli::try_parse_from([
            "repo-sync",
            "-j",
            "0",
            "sync",
            "-f",
            "repos.txt",
            "-o",
            "out",
        ]);

        assert!(result.is_err(), "-j 0 must be rejected at parse time");
    }

    /// `-j 8` must be accepted and parsed into `Some(8)`.
    #[test]
    fn jobs_accepts_a_positive_value() {
        let cli = Cli::try_parse_from([
            "repo-sync",
            "-j",
            "8",
            "sync",
            "-f",
            "repos.txt",
            "-o",
            "out",
        ])
        .expect("-j 8 must be accepted");

        assert_eq!(cli.jobs, Some(NonZeroUsize::new(8).unwrap()));
    }

    /// Omitting `-j` entirely must parse fine and leave `jobs` unset, so
    /// the run falls back to rayon's own default.
    #[test]
    fn jobs_defaults_to_none() {
        let cli = Cli::try_parse_from(["repo-sync", "sync", "-f", "repos.txt", "-o", "out"])
            .expect("sync without -j must be accepted");

        assert_eq!(cli.jobs, None);
    }
}
