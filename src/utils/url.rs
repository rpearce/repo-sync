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
///
/// - `url`: repo-list entry to normalize
pub fn normalize(url: &str) -> String {
    // Upgrade a leading "http://" to "https://"
    if let Some(rest) = url.strip_prefix("http://") {
        return format!("https://{}", rest);
    }

    // If git can already use it as-is (a URL with a scheme, an scp-style
    // remote or a local path), leave it
    if url.contains("://") || is_scp_like(url) || url.starts_with(['/', '.']) {
        url.to_string()
    } else {
        // Otherwise, prepend "https://"
        format!("https://{}", url)
    }
}

/// Detect scp-like remotes such as `git@github.com:user/repo.git` or
/// `host:repo.git`: no `"://"` (checked by the caller), with a `:` that
/// appears before the first `/`, or a `:` and no `/` at all.
/// - `url`: repo-list entry, already known not to contain `"://"`
fn is_scp_like(url: &str) -> bool {
    scp_colon_index(url).is_some()
}

/// Find the separator colon of an scp-like remote (e.g.
/// `git@github.com:user/repo.git` or `host:repo.git`): a `:` that appears
/// before the first `/`, or a `:` with no `/` at all. `None` if `url`
/// doesn't have that shape.
/// - `url`: repo-list entry, already known not to contain `"://"`
fn scp_colon_index(url: &str) -> Option<usize> {
    let colon_idx = url.find(':')?;
    match url.find('/') {
        None => Some(colon_idx),
        Some(slash_idx) if colon_idx < slash_idx => Some(colon_idx),
        Some(_) => None,
    }
}

/// Derive a repository's local directory name from `url` (its
/// **normalized** form — see `normalize`).
///
/// Finds `url`'s path (skipping the scheme and, for `scheme://host/...`
/// forms, the host — so the `//` after a scheme is never mistaken for a
/// path separator), takes that path's last `/`-separated segment, and
/// strips one trailing `.git` suffix (never a substring match: a
/// `.git`-free name like `rpearce.github.io` must come through whole).
/// Rejects `""` (no path at all, e.g. `https://host/`, which
/// `Path::join` would otherwise resolve to the output directory itself),
/// `"."` and `".."` (which would resolve to the output directory or its
/// parent).
/// - `url`: a normalized repo-list entry (see `normalize`)
pub fn repo_name(url: &str) -> Option<&str> {
    let path = match url.find("://") {
        Some(scheme_end) => {
            let after_scheme = &url[scheme_end + 3..];
            match after_scheme.find('/') {
                Some(host_end) => &after_scheme[host_end..],
                None => "",
            }
        }
        None => match scp_colon_index(url) {
            Some(colon_idx) => &url[colon_idx + 1..],
            None => url,
        },
    };

    let trimmed = path.trim_end_matches('/');
    let segment = trimmed.rsplit('/').next().unwrap_or("");
    let name = segment.strip_suffix(".git").unwrap_or(segment);

    match name {
        "" | "." | ".." => None,
        _ => Some(name),
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
    fn normalize_with_http_prefix_and_port() {
        let input = "http://example.com:8080/user/repo.git";
        let expected = "https://example.com:8080/user/repo.git";
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

    #[test]
    fn repo_name_full_github_pages_domain_is_not_mangled() {
        let input = "https://github.com/rpearce/rpearce.github.io";
        assert_eq!(repo_name(input), Some("rpearce.github.io"));
    }

    #[test]
    fn repo_name_strips_git_suffix() {
        let input = "https://github.com/u/repo.git";
        assert_eq!(repo_name(input), Some("repo"));
    }

    #[test]
    fn repo_name_ignores_trailing_slash() {
        let input = "https://github.com/u/repo/";
        assert_eq!(repo_name(input), Some("repo"));
    }

    #[test]
    fn repo_name_ignores_trailing_slash_after_git_suffix() {
        let input = "https://github.com/u/repo.git/";
        assert_eq!(repo_name(input), Some("repo"));
    }

    #[test]
    fn repo_name_scp_like_with_owner() {
        let input = "git@github.com:u/repo.git";
        assert_eq!(repo_name(input), Some("repo"));
    }

    #[test]
    fn repo_name_scp_like_without_owner() {
        let input = "git@host:repo.git";
        assert_eq!(repo_name(input), Some("repo"));
    }

    #[test]
    fn repo_name_file_url() {
        let input = "file:///tmp/x/repo";
        assert_eq!(repo_name(input), Some("repo"));
    }

    #[test]
    fn repo_name_rejects_parent_dir_segment() {
        let input = "https://github.com/u/..";
        assert_eq!(repo_name(input), None);
    }

    #[test]
    fn repo_name_rejects_current_dir_segment() {
        let input = "https://github.com/u/.";
        assert_eq!(repo_name(input), None);
    }

    #[test]
    fn repo_name_rejects_bare_host_with_no_path() {
        let input = "https://github.com/";
        assert_eq!(repo_name(input), None);
    }
}
