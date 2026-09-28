/// Normalize a repo-list entry into something `git` can use directly.
///
/// Rules, in order:
/// - `http://` is upgraded to `https://`.
/// - Anything containing `"://"` (e.g. already `https://`, `ssh://`,
///   `git://`, `file://`) is returned unchanged.
/// - scp-like entries (no `"://"`, with a `:` before the first `/`, or a
///   `:` and no `/` at all — e.g. `git@github.com:user/repo.git` or
///   `host:repo.git`) are returned unchanged.
/// - Local paths starting with `/` or `.` are returned unchanged.
/// - Anything else is treated as a bare `host/owner/repo` entry and
///   prefixed with `https://`.
/// - `url`: repo-list entry to normalize
pub fn normalize(url: &str) -> String {
    if let Some(rest) = url.strip_prefix("http://") {
        return format!("https://{}", rest);
    }

    if url.contains("://") {
        return url.to_string();
    }

    if is_scp_like(url) {
        return url.to_string();
    }

    if url.starts_with('/') || url.starts_with('.') {
        return url.to_string();
    }

    format!("https://{}", url)
}

/// Detect scp-like remotes such as `git@github.com:user/repo.git` or
/// `host:repo.git`: no `"://"` (checked by the caller), with a `:` that
/// appears before the first `/`, or a `:` and no `/` at all.
/// - `url`: repo-list entry, already known not to contain `"://"`
fn is_scp_like(url: &str) -> bool {
    match url.find(':') {
        None => false,
        Some(colon_idx) => match url.find('/') {
            None => true,
            Some(slash_idx) => colon_idx < slash_idx,
        },
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

    #[test]
    fn normalize_scp_like_ssh_shorthand() {
        let input = "git@github.com:user/repo.git";
        let expected = "git@github.com:user/repo.git";
        assert_eq!(normalize(input), expected);
    }

    #[test]
    fn normalize_ssh_url() {
        let input = "ssh://git@github.com/user/repo.git";
        let expected = "ssh://git@github.com/user/repo.git";
        assert_eq!(normalize(input), expected);
    }

    #[test]
    fn normalize_git_url() {
        let input = "git://example.com/repo.git";
        let expected = "git://example.com/repo.git";
        assert_eq!(normalize(input), expected);
    }

    #[test]
    fn normalize_file_url() {
        let input = "file:///tmp/remotes/repo.git";
        let expected = "file:///tmp/remotes/repo.git";
        assert_eq!(normalize(input), expected);
    }

    #[test]
    fn normalize_local_path() {
        let input = "/tmp/remotes/repo.git";
        let expected = "/tmp/remotes/repo.git";
        assert_eq!(normalize(input), expected);
    }
}
