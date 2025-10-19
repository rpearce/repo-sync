use std::fs;

use clap;
use rayon::prelude::*;

use crate::config::Config;
use crate::git::clone_repo;

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
pub fn run(config: &Config) {
    let content = fs::read_to_string(&config.repos_file).expect("Failed to read repo list file");
    let repos: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    if config.verbose {
        println!(
            "Cloning {} repositories into {:?}",
            repos.len(),
            &config.output_dir
        );
    }

    repos.par_iter().for_each(|url| clone_repo(url, config));
}
