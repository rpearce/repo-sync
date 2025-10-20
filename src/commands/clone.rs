use std::{fs, path::Path};

use clap;
use rayon::prelude::*;

use crate::config::Config;
use crate::git::clone::git_clone;
use crate::utils::url::normalize;

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

/// Clone a repository only if it doesn't already exist.
/// - `url`: repository URL
/// - `base_dir`: directory where the repo should be cloned
/// - `config`: command configuration
pub fn clone_repo(url: &str, config: &Config) {
    let url = normalize(url);
    let name = url.split('/').next_back().unwrap().replace(".git", "");
    let path = Path::new(&config.output_dir).join(&name);

    if path.exists() {
        if config.verbose {
            println!("Skipping {}, already exists", name);
        }
    } else if let Err(e) = git_clone(&url, &path, config) {
        eprintln!("Error cloning {}: {}", url, e);
    }
}

/// Runs the `clone` command.
/// - `config`: command configuration
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
