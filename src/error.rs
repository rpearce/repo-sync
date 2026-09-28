use std::{fmt, io};

/// A single repository operation's failure, with enough context to report
/// clearly regardless of which repo it was for or what went wrong. Each
/// variant owns its exact user-facing wording, so a caller just prints
/// `{e}`. New failure kinds (e.g. an existing directory that isn't a git
/// repository, an entry with no usable directory name, or a directory-name
/// collision) are added here as new variants.
#[derive(Debug)]
pub enum RepoError {
    /// `git clone` failed for `url`.
    Clone { url: String, source: io::Error },
    /// Syncing an existing clone's branches failed for `url`.
    Sync { url: String, source: io::Error },
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoError::Clone { url, source } => write!(f, "Error cloning {url}: {source}"),
            RepoError::Sync { url, source } => {
                write!(f, "Error syncing branches in {url}: {source}")
            }
        }
    }
}

impl std::error::Error for RepoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepoError::Clone { source, .. } | RepoError::Sync { source, .. } => Some(source),
        }
    }
}
