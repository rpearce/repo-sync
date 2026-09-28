use std::process::ExitCode;

use clap;
use rayon::prelude::*;

use crate::commands::{partition_collisions, read_repo_list, report};
use crate::config::Config;
use crate::error::RepoError;
use crate::git::sync::sync_repo;
use crate::utils::repo_list::parse_repo_list;

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
    let content = match read_repo_list(config) {
        Ok(content) => content,
        Err(code) => return code,
    };
    let repos = parse_repo_list(&content);
    let (to_run, failures) = partition_collisions(&repos);

    if config.verbose {
        println!(
            "Syncing {} repositories in {:?}",
            repos.len(),
            config.output_dir
        );
    }

    let mut results: Vec<Result<(), RepoError>> = to_run
        .par_iter()
        .map(|url| sync_repo(url, config))
        .collect();
    results.extend(failures.into_iter().map(Err));

    report("Synced", &results, config.verbose)
}
