use std::{io, path::PathBuf, process};

use crate::config::Config;

/// Clone a Git repository from `url` into `path`.
/// - `url`: repository URL
/// - `path`: local repository target directory
/// - `config`: command configuration
pub fn git_clone(url: &str, path: &PathBuf, config: &Config) -> io::Result<()> {
    // Spawn a `git clone <url> <path>` process and wait for it to finish
    let mut clone_cmd = process::Command::new("git");
    clone_cmd.arg("clone").arg(url).arg(path);
    if !config.verbose {
        clone_cmd.arg("--quiet");
    }

    let status = clone_cmd.status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("git clone failed for {:?}", url)))
    }
}
