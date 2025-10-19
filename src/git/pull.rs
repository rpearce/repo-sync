use std::{io, path::PathBuf, process};

use crate::config::Config;

/// Pull updates in the Git repository at `path`.
/// - `path`: local repository directory
/// - `config`: command configuration
pub fn git_pull(path: &PathBuf, config: &Config) -> io::Result<()> {
    // Spawn a `git -C <path> pull` process to pull the latest changes
    // `-C <path>` tells Git to operate in the specified directory
    let mut pull_cmd = process::Command::new("git");
    pull_cmd.arg("-C").arg(path).arg("pull");
    if !config.verbose {
        pull_cmd.arg("--quiet");
    }

    let status = pull_cmd.status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("git pull failed in {:?}", path)))
    }
}
