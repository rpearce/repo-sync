/// Normalize a repository URL to always use HTTPS.
/// - `url`: URL
pub fn normalize(url: &str) -> String {
    // Remove leading "http://" if present
    let url = url.strip_prefix("http://").unwrap_or(url);

    // If it already starts with "https://", leave it
    if url.starts_with("https://") {
        url.to_string()
    } else {
        // Otherwise, prepend "https://"
        format!("https://{}", url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_with_http_prefix() {
        let input = "http://github.com/user/repo.git";
        let expected = "https://github.com/user/repo.git";
        assert_eq!(normalize(input), expected);
    }

    #[test]
    fn normalize_without_prefix() {
        let input = "github.com/user/repo.git";
        let expected = "https://github.com/user/repo.git";
        assert_eq!(normalize(input), expected);
    }

    #[test]
    fn normalize_already_https() {
        let input = "https://github.com/user/repo.git";
        let expected = "https://github.com/user/repo.git";
        assert_eq!(normalize(input), expected);
    }
}
