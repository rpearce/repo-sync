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
    /// Two or more *different* normalized URLs derive the same directory
    /// name (e.g. `.../a/dotfiles` and `.../b/dotfiles` both resolve to
    /// `<out>/dotfiles`). Detected up front, before the parallel
    /// clone/sync phase, instead of letting the entries race to write the
    /// same path; none of them are run. `entries` names every entry in
    /// the collision, as normalized URLs (see
    /// `crate::utils::url::normalize`) — matching entries that normalize
    /// to the exact same URL are a separate, non-error case (a warning;
    /// see `crate::commands::partition_collisions`), not a
    /// `DuplicateName`. One `DuplicateName` is produced per *collision*,
    /// not per colliding entry; see `failed_entries` for how `report`
    /// still counts every named entry toward the failed total.
    DuplicateName { name: String, entries: Vec<String> },
}

impl RepoError {
    /// How many repo-list entries this failure represents, for
    /// `crate::commands::report`'s failed count. Every variant is one
    /// entry's own failure (1), except `DuplicateName`:
    /// `crate::commands::partition_collisions` emits exactly one of those
    /// per *collision* (not per colliding entry), so its weight is the
    /// number of entries it names instead.
    pub(crate) fn failed_entries(&self) -> usize {
        match self {
            RepoError::DuplicateName { entries, .. } => entries.len(),
            RepoError::Clone { .. }
            | RepoError::Sync { .. }
            | RepoError::NoDirectoryName { .. }
            | RepoError::NotAGitRepository { .. } => 1,
        }
    }
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
            RepoError::DuplicateName { name, entries } => {
                write!(
                    f,
                    "'{name}' is used by multiple entries: {}",
                    entries.join(", ")
                )
            }
        }
    }
}

impl std::error::Error for RepoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepoError::Clone { source, .. } | RepoError::Sync { source, .. } => Some(source),
            RepoError::NoDirectoryName { .. }
            | RepoError::NotAGitRepository { .. }
            | RepoError::DuplicateName { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_entries_is_one_for_ordinary_failures() {
        let clone_err = RepoError::Clone {
            url: "https://host/r".to_string(),
            source: io::Error::other("boom"),
        };
        let no_name_err = RepoError::NoDirectoryName {
            entry: "https://host/".to_string(),
        };
        assert_eq!(clone_err.failed_entries(), 1);
        assert_eq!(no_name_err.failed_entries(), 1);
    }

    #[test]
    fn failed_entries_counts_every_entry_named_in_a_collision() {
        let err = RepoError::DuplicateName {
            name: "dotfiles".to_string(),
            entries: vec![
                "https://host/a/dotfiles".to_string(),
                "https://host/b/dotfiles".to_string(),
                "https://host/c/dotfiles".to_string(),
            ],
        };
        assert_eq!(err.failed_entries(), 3);
    }
}
