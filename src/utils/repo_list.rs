/// Parse a repo-list file's raw contents into individual entries.
///
/// - strips a leading UTF-8 BOM, if present;
/// - splits into lines (`str::lines` already strips CRLF's trailing `\r`);
/// - trims each line;
/// - drops blank lines and lines starting with `#` (comments), checked
///   after trimming so an indented `#` is still recognized as a comment.
/// - `content`: raw repo-list file contents
pub fn parse_repo_list(content: &str) -> Vec<&str> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_entries_per_the_table() {
        let cases: &[(&str, &str, &[&str])] = &[
            ("blank lines are dropped", "a\n\nb\n", &["a", "b"]),
            ("comment lines are dropped", "# comment\na\n", &["a"]),
            (
                "leading and trailing whitespace is trimmed",
                "  a  \n\tb\t\n",
                &["a", "b"],
            ),
            (
                "comments are recognized only after trimming",
                "  # comment\na\n",
                &["a"],
            ),
            ("CRLF line endings are handled", "a\r\nb\r\n", &["a", "b"]),
            (
                "a leading UTF-8 BOM is stripped",
                "\u{feff}a\nb\n",
                &["a", "b"],
            ),
        ];

        for (description, input, expected) in cases {
            assert_eq!(parse_repo_list(input), *expected, "case: {description}");
        }
    }
}
