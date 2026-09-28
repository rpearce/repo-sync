use std::{io, path::Path};

use crate::config::Config;
use crate::git::command;

/// Clone a Git repository from `url` into `path`.
/// - `url`: repository URL
/// - `path`: local repository target directory
/// - `config`: command configuration
pub fn git_clone(url: &str, path: &Path, config: &Config) -> io::Result<()> {
    command::run_clone(url, path, config.verbose)
}
