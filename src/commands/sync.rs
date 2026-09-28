use std::{fs, process::ExitCode};

use clap;
use rayon::prelude::*;

use crate::config::Config;
use crate::error::RepoError;
use crate::git::sync::sync_repo;

/// Returns the `clap::Command` spec for the `sync` subcommand.
pub fn command() -> clap::Command {
    clap::Command::new("sync")
        .about("Sync existing repositories (fetch + branch fast-forward)")
        .arg(
            clap::Arg::new("file")
                .short('f')
                .long("file")
                .help("Repository list file")
                .required(true),
        )
        .arg(
            clap::Arg::new("out")
                .short('o')
                .long("out")
                .help("Output directory")
                .required(true),
        )
        .arg(
            clap::Arg::new("verbose")
                .short('v')
                .long("verbose")
                .help("Enable verbose output")
                .action(clap::ArgAction::SetTrue),
        )
}

/// Runs the `sync` command.
/// - `config`: command configuration
///
/// Returns `ExitCode::FAILURE` (without syncing anything) if the repo
/// list can't be read, or if any repository fails to clone or sync.
pub fn run(config: &Config) -> ExitCode {
    let content = match fs::read_to_string(&config.repos_file) {
        Ok(content) => content,
        Err(e) => {
            eprintln!(
                "error: cannot read repo list '{}': {e}",
                config.repos_file.display()
            );
            return ExitCode::FAILURE;
        }
    };
    let repos: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    if config.verbose {
        println!(
            "Syncing {} repositories in {:?}",
            repos.len(),
            config.output_dir
        );
    }

    let results: Vec<Result<(), RepoError>> =
        repos.par_iter().map(|url| sync_repo(url, config)).collect();

    for result in &results {
        if let Err(e) = result {
            eprintln!("{e}");
        }
    }

    let total = results.len();
    let failed = results.iter().filter(|r| r.is_err()).count();
    let ok = total - failed;
    let summary = format!("Synced {total} repositories: {ok} ok, {failed} failed");

    if failed > 0 {
        eprintln!("{summary}");
        return ExitCode::FAILURE;
    }

    if config.verbose {
        println!("{summary}");
    }

    ExitCode::SUCCESS
}
