use std::fs;

use clap;
use rayon::prelude::*;

use crate::config::Config;
use crate::git::sync_repo;

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
pub fn run(config: &Config) {
    let content = fs::read_to_string(&config.repos_file).expect("Failed to read repo list file");
    let repos: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    if config.verbose {
        println!(
            "Syncing {} repositories in {:?}",
            repos.len(),
            &config.output_dir
        );
    }

    repos.par_iter().for_each(|url| sync_repo(url, config));
}
