use std::num::NonZeroUsize;
use std::path::PathBuf;

/// Configuration for repo-sync operations
#[derive(Debug, Clone)]
pub struct Config {
    /// Path to the file containing repository URLs
    pub repos_file: PathBuf,

    /// Output directory where repositories will be cloned/synced
    pub output_dir: PathBuf,

    /// Whether to operate in verbose mode
    pub verbose: bool,

    /// `-j`/`--jobs`: how many repositories to process in parallel.
    /// `None` means "use rayon's own default" (see
    /// `crate::commands::run_in_pool`).
    pub jobs: Option<NonZeroUsize>,
}

impl Config {
    /// Create a new configuration
    pub fn new<P: Into<PathBuf>>(repos_file: P, output_dir: P) -> Self {
        Self {
            repos_file: repos_file.into(),
            output_dir: output_dir.into(),
            verbose: false,
            jobs: None,
        }
    }

    /// Set verbose mode
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Set the parallelism bound (`-j`/`--jobs`); `None` keeps rayon's
    /// own default.
    pub fn with_jobs(mut self, jobs: Option<NonZeroUsize>) -> Self {
        self.jobs = jobs;
        self
    }
}
