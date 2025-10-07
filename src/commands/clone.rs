use std::fs;

use clap;
use rayon::prelude::*;

use crate::config::Config;
use crate::error::{RepoSyncError, Result};
use crate::git::clone_repo;
use crate::repo_list::parse_repo_file;
use crate::utils::output::RepoStatusPrinter;

/// Returns the `clap::Command` spec for the `clone` subcommand.
pub fn command() -> clap::Command {
    clap::Command::new("clone")
        .about("Clone repositories from a file into an output directory")
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

/// Runs the `clone` command.
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

    printer.summary("Cloning", repos.len(), config.output_dir_str()?);

    // Clone repositories with simplified error handling
    let failed_count = repos
        .par_iter()
        .map(|url| {
            match config.output_dir_str() {
                Ok(output_dir) => {
                    if let Err(e) = clone_repo(url, output_dir, config) {
                        eprintln!("Error cloning {}: {}", url, e);
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

    printer.final_summary("clone", failed_count, repos.len());

    Ok(())
}
