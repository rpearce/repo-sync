use std::fs;

use clap;
use rayon::prelude::*;

use crate::config::Config;
use crate::error::{RepoSyncError, Result};
use crate::git::sync_repo;
use crate::repo_list::parse_repo_file;
use crate::utils::output::RepoStatusPrinter;

/// Returns the `clap::Command` spec for the `sync` subcommand.
pub fn command() -> clap::Command {
    clap::Command::new("sync")
        .about("Sync existing repositories (pull + branch updates)")
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
                .help("Repository directory")
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
pub fn run(config: &Config) -> Result<()> {
    // Ensure output directory exists
    fs::create_dir_all(&config.output_dir).map_err(|e| {
        RepoSyncError::directory(
            config.output_dir.display().to_string(),
            format!("Failed to create output directory: {}", e)
        )
    })?;

    // Read and parse repository list
    let repos = parse_repo_file(config.repos_file_str()?)?;
    let printer = RepoStatusPrinter::new(config);

    printer.summary("Syncing", repos.len(), config.output_dir_str()?);

    // Sync repositories with simplified error handling
    let failed_count = repos
        .par_iter()
        .map(|url| {
            match config.output_dir_str() {
                Ok(output_dir) => {
                    if let Err(e) = sync_repo(url, output_dir, config) {
                        eprintln!("Error syncing {}: {}", url, e);
                        1
                    } else {
                        0
                    }
                }
                Err(e) => {
                    eprintln!("Error with output directory path: {}", e);
                    1
                }
            }
        })
        .sum::<usize>();

    printer.final_summary("sync", failed_count, repos.len());

    Ok(())
}
