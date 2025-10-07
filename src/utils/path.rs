use crate::error::{RepoSyncError, Result};

/// Extract repository name from a pre-validated URL.
///
/// Examples:
/// - "https://github.com/user/repo.git" -> "repo"
/// - "https://gitlab.com/group/project" -> "project"
///
/// # Arguments
/// * `url` - Pre-validated repository URL
///
/// # Errors
/// Returns `RepoSyncError::Config` if URL parsing fails (should not happen for pre-validated URLs)
pub fn extract_repo_name(url: &str) -> Result<String> {
    let name = url
        .split('/')
        .last()
        .ok_or_else(|| {
            RepoSyncError::config(format!(
                "Internal error: Cannot extract repository name from pre-validated URL: {}",
                url
            ))
        })?
        .replace(".git", "");

    // Additional validation: name shouldn't be empty after processing
    if name.is_empty() {
        return Err(RepoSyncError::config(format!(
            "Internal error: Extracted empty repository name from URL: {}",
            url
        )));
    }

    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_repo_name_with_git_suffix() {
        let result = extract_repo_name("https://github.com/user/repo.git").unwrap();
        assert_eq!(result, "repo");
    }

    #[test]
    fn test_extract_repo_name_without_git_suffix() {
        let result = extract_repo_name("https://gitlab.com/group/project").unwrap();
        assert_eq!(result, "project");
    }

    #[test]
    fn test_extract_repo_name_nested_path() {
        let result = extract_repo_name("https://github.com/org/team/repo.git").unwrap();
        assert_eq!(result, "repo");
    }

    #[test]
    fn test_extract_repo_name_empty_url() {
        let result = extract_repo_name("");
        assert!(result.is_err());
    }
}