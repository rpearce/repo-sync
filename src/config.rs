use std::path::PathBuf;
use crate::error::{RepoSyncError, Result};

/// Configuration for repo-sync operations
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the file containing repository URLs
    pub repos_file: PathBuf,
    /// Output directory where repositories will be cloned/synced
    pub output_dir: PathBuf,
    /// Whether to operate in verbose mode
    pub verbose: bool,
}

impl Config {
    /// Create a new configuration
    pub fn new<P: Into<PathBuf>>(repos_file: P, output_dir: P) -> Self {
        Self {
            repos_file: repos_file.into(),
            output_dir: output_dir.into(),
            verbose: false,
        }
    }

    /// Set verbose mode
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Get the repos file path as a string slice
    pub fn repos_file_str(&self) -> Result<&str> {
        self.repos_file.to_str().ok_or_else(|| {
            RepoSyncError::config(format!(
                "Repository file path contains invalid UTF-8: {}",
                self.repos_file.display()
            ))
        })
    }

    /// Get the output directory path as a string slice
    pub fn output_dir_str(&self) -> Result<&str> {
        self.output_dir.to_str().ok_or_else(|| {
            RepoSyncError::config(format!(
                "Output directory path contains invalid UTF-8: {}",
                self.output_dir.display()
            ))
        })
    }
}