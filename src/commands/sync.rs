use std::fs;

use clap;
use rayon::prelude::*;

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
                .help("Repository directory")
                .required(true),
        )
}

/// Runs the `sync` command.
pub fn run(file: &str, out: &str) {
    let content = fs::read_to_string(file).expect("Failed to read repo list file");
    let repos: Vec<&str> = content.lines().filter(|l| !l.trim().is_empty()).collect();

    println!("Syncing {} repositories in {}", repos.len(), out);
    repos.par_iter().for_each(|url| sync_repo(url, out));
}
