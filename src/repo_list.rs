use std::fs;
use crate::error::{RepoSyncError, Result};
use crate::utils::validation::normalize_and_validate_url;

/// Parse a repository list file, returning validated and normalized repository URLs
pub fn parse_repo_file(file_path: &str) -> Result<Vec<String>> {
    let content = fs::read_to_string(file_path).map_err(|e| {
        RepoSyncError::config(format!("Failed to read repo list file '{}': {}", file_path, e))
    })?;

    let mut repos = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Validate and normalize the URL
        match normalize_and_validate_url(line) {
            Ok(normalized_url) => repos.push(normalized_url),
            Err(_) => {
                return Err(RepoSyncError::file_format(
                    file_path,
                    line_num + 1, // Convert to 1-based line numbers
                    line
                ));
            }
        }
    }

    if repos.is_empty() {
        return Err(RepoSyncError::config("No repositories found in the repo list file"));
    }

    Ok(repos)
}