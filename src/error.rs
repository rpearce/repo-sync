use std::{fmt, io};

/// Result type alias for repo-sync operations
pub type Result<T> = std::result::Result<T, RepoSyncError>;

/// Error types for repo-sync operations
#[derive(Debug)]
pub enum RepoSyncError {
    /// I/O error (file operations, process spawning, etc.)
    Io(io::Error),

    /// Git operation failed
    Git { operation: String, message: String },

    /// Invalid repository URL
    InvalidUrl(String),

    /// Repository file parsing error
    FileFormat {
        file: String,
        line: usize,
        content: String,
    },

    /// Directory creation or validation error
    Directory { path: String, message: String },

    /// Configuration error
    Config(String),
}

impl fmt::Display for RepoSyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RepoSyncError::Io(err) => write!(f, "I/O error: {}", err),
            RepoSyncError::Git { operation, message } => {
                write!(f, "Git {} failed: {}", operation, message)
            }
            RepoSyncError::InvalidUrl(url) => write!(f, "Invalid repository URL: {}", url),
            RepoSyncError::FileFormat {
                file,
                line,
                content,
            } => {
                write!(f, "Invalid line {} in file {}: '{}'", line, file, content)
            }
            RepoSyncError::Directory { path, message } => {
                write!(f, "Directory error for '{}': {}", path, message)
            }
            RepoSyncError::Config(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for RepoSyncError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RepoSyncError::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<io::Error> for RepoSyncError {
    fn from(err: io::Error) -> Self {
        RepoSyncError::Io(err)
    }
}

impl RepoSyncError {
    /// Create a new Git error
    pub fn git<O: Into<String>, M: Into<String>>(operation: O, message: M) -> Self {
        RepoSyncError::Git {
            operation: operation.into(),
            message: message.into(),
        }
    }

    /// Create a new invalid URL error
    pub fn invalid_url<S: Into<String>>(url: S) -> Self {
        RepoSyncError::InvalidUrl(url.into())
    }

    /// Create a new file format error
    pub fn file_format<S: Into<String>>(file: S, line: usize, content: S) -> Self {
        RepoSyncError::FileFormat {
            file: file.into(),
            line,
            content: content.into(),
        }
    }

    /// Create a new directory error
    pub fn directory<P: Into<String>, M: Into<String>>(path: P, message: M) -> Self {
        RepoSyncError::Directory {
            path: path.into(),
            message: message.into(),
        }
    }

    /// Create a new configuration error
    pub fn config<S: Into<String>>(message: S) -> Self {
        RepoSyncError::Config(message.into())
    }
}

