use std::{path::Path, process::ExitCode};

use clap;
use rayon::prelude::*;

use crate::commands::{partition_collisions, read_repo_list, report};
use crate::config::Config;
use crate::error::RepoError;
use crate::git::clone::git_clone;
use crate::utils::repo_list::parse_repo_list;
use crate::utils::url::{normalize, repo_name};

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
pub fn clone_repo(url: &str, config: &Config) -> Result<(), RepoError> {
    let url = normalize(url);
    let name = repo_name(&url).ok_or_else(|| RepoError::NoDirectoryName { entry: url.clone() })?;
    let path = Path::new(&config.output_dir).join(name);

    if path.exists() {
        if config.verbose {
            println!("Skipping {}, already exists", name);
        }
        Ok(())
    } else {
        git_clone(&url, &path, config).map_err(|source| RepoError::Clone { url, source })
    }
}

/// Runs the `clone` command.
/// - `config`: command configuration
///
/// Returns `ExitCode::FAILURE` (without cloning anything) if the repo
/// list can't be read, or if any repository fails to clone.
pub fn run(config: &Config) -> ExitCode {
    let content = match read_repo_list(config) {
        Ok(content) => content,
        Err(code) => return code,
    };
    let repos = parse_repo_list(&content);
    let (to_run, failures) = partition_collisions(&repos);

    if config.verbose {
        println!(
            "Cloning {} repositories into {:?}",
            repos.len(),
            config.output_dir
        );
    }

    let mut results: Vec<Result<(), RepoError>> = to_run
        .par_iter()
        .map(|url| clone_repo(url, config))
        .collect();
    results.extend(failures.into_iter().map(Err));

    report("Cloned", &results, config.verbose)
}
