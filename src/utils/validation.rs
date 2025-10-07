use crate::error::{RepoSyncError, Result};

/// Normalize a repository URL to always use HTTPS and validate it.
///
/// This function:
/// 1. Trims whitespace
/// 2. Upgrades HTTP to HTTPS
/// 3. Adds HTTPS prefix if missing
/// 4. Validates the URL has proper domain/repo format
///
/// # Arguments
/// * `url` - Raw URL string to normalize and validate
///
/// # Returns
/// * `Ok(String)` - Normalized HTTPS URL
/// * `Err(RepoSyncError)` - If URL is invalid or empty
///
/// # Examples
/// ```
/// use repo_sync::utils::validation::normalize_and_validate_url;
///
/// let url = normalize_and_validate_url("github.com/user/repo").unwrap();
/// assert_eq!(url, "https://github.com/user/repo");
///
/// let url = normalize_and_validate_url("http://gitlab.com/project.git").unwrap();
/// assert_eq!(url, "https://gitlab.com/project.git");
/// ```
pub fn normalize_and_validate_url(url: &str) -> Result<String> {
    let url = url.trim();

    // Check for empty or whitespace-only URLs
    if url.is_empty() {
        return Err(RepoSyncError::invalid_url("URL cannot be empty"));
    }

    // Remove leading "http://" if present (upgrade to HTTPS)
    let url = url.strip_prefix("http://").unwrap_or(url);

    // Build the normalized URL
    let normalized = if url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://{}", url)
    };

    // Basic validation: must contain at least one slash (domain/repo pattern)
    // Allow both single-level (domain.com/repo) and multi-level (domain.com/user/repo) repos
    let path_part = normalized.strip_prefix("https://").unwrap_or(&normalized);
    let slash_count = path_part.matches('/').count();

    if slash_count < 1 {
        return Err(RepoSyncError::invalid_url(format!(
            "Malformed URL: must have format 'domain.com/repo': {}",
            url
        )));
    }

    Ok(normalized)
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_https_url() {
        let result = normalize_and_validate_url("https://github.com/user/repo").unwrap();
        assert_eq!(result, "https://github.com/user/repo");
    }

    #[test]
    fn test_normalize_http_url() {
        let result = normalize_and_validate_url("http://github.com/user/repo").unwrap();
        assert_eq!(result, "https://github.com/user/repo");
    }

    #[test]
    fn test_normalize_bare_url() {
        let result = normalize_and_validate_url("github.com/user/repo").unwrap();
        assert_eq!(result, "https://github.com/user/repo");
    }

    #[test]
    fn test_normalize_with_git_suffix() {
        let result = normalize_and_validate_url("gitlab.com/project.git").unwrap();
        assert_eq!(result, "https://gitlab.com/project.git");
    }

    #[test]
    fn test_normalize_empty_url() {
        let result = normalize_and_validate_url("");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_whitespace_url() {
        let result = normalize_and_validate_url("  ");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_no_slash() {
        let result = normalize_and_validate_url("github.com");
        assert!(result.is_err());
    }

    #[test]
    fn test_normalize_nested_path() {
        let result = normalize_and_validate_url("github.com/org/team/repo").unwrap();
        assert_eq!(result, "https://github.com/org/team/repo");
    }

}