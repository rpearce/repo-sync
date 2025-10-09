use std::fs;

use clap;
use rayon::prelude::*;

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
}

/// Runs the `clone` command.
pub fn run(file: &str, out: &str) {
    let content = fs::read_to_string(file).expect("Failed to read repo list file");
    let repos: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    println!("Cloning {} repositories into {}", repos.len(), out);
    repos.par_iter().for_each(|url| clone_repo(url, out));
}
