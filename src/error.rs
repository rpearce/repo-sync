use std::path::PathBuf;
use std::{fmt, io};

/// A single repository operation's failure, with enough context to report
/// clearly regardless of which repo it was for or what went wrong. Each
/// variant owns its exact user-facing wording, so a caller just prints
/// `{e}`. New failure kinds (e.g. a directory-name collision) are added
/// here as new variants.
#[derive(Debug)]
pub enum RepoError {
    /// `git clone` failed for `url`.
    Clone { url: String, source: io::Error },
    /// Syncing an existing clone's branches failed for `url`.
    Sync { url: String, source: io::Error },
    /// The repo-list entry `entry` (after normalization) has no path
    /// segment usable as a directory name — e.g. a bare host with no
    /// path, or one that resolves to `.` or `..`. See
    /// `crate::utils::url::repo_name`.
    NoDirectoryName { entry: String },
    /// `path` already exists but isn't a git repository (no `.git` entry
    /// under it). Running git there anyway would let it discover and
    /// silently operate on whatever *enclosing* repository it walks up
    /// to find instead.
    NotAGitRepository { path: PathBuf },
}

impl fmt::Display for RepoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoError::Clone { url, source } => write!(f, "Error cloning {url}: {source}"),
            RepoError::Sync { url, source } => {
                write!(f, "Error syncing branches in {url}: {source}")
            }
            RepoError::NoDirectoryName { entry } => {
                write!(f, "cannot determine a directory name for '{entry}'")
            }
            RepoError::NotAGitRepository { path } => {
                write!(f, "{} exists but is not a git repository", path.display())
            }
        }
    }
}

impl std::error::Error for RepoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepoError::Clone { source, .. } | RepoError::Sync { source, .. } => Some(source),
            RepoError::NoDirectoryName { .. } | RepoError::NotAGitRepository { .. } => None,
        }
    }
}
