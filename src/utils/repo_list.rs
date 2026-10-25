use std::collections::HashMap;

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

/// Group `entries` by the directory name each one derives, and return
/// only the names claimed by more than one entry (a "collision"): each
/// result pairs the name with every entry that produced it, in the order
/// those entries appear in `entries`. Collisions are themselves ordered
/// by the name's first appearance, so the result is deterministic no
/// matter what order the caller computed `entries` in (e.g. after a
/// parallel pre-pass).
/// - `entries`: `(entry, name)` pairs, one per repo-list entry that has a
///   usable directory name (see `crate::utils::url::repo_name`); entries
///   with no usable name don't participate and are the caller's concern
pub fn find_collisions<'a>(entries: &[(&'a str, &'a str)]) -> Vec<(&'a str, Vec<&'a str>)> {
    let mut order: Vec<&'a str> = Vec::new();
    let mut groups: HashMap<&'a str, Vec<&'a str>> = HashMap::new();

    for &(entry, name) in entries {
        let group = groups.entry(name).or_default();
        if group.is_empty() {
            order.push(name);
        }
        group.push(entry);
    }

    order
        .into_iter()
        .filter_map(|name| {
            let group = groups.remove(name)?;
            (group.len() > 1).then_some((name, group))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_collisions_is_empty_when_all_names_are_unique() {
        let entries = [("e1", "a"), ("e2", "b")];
        assert_eq!(find_collisions(&entries), Vec::new());
    }

    #[test]
    fn find_collisions_is_empty_for_no_entries() {
        let entries: [(&str, &str); 0] = [];
        assert_eq!(find_collisions(&entries), Vec::new());
    }

    #[test]
    fn find_collisions_reports_a_name_shared_by_two_entries() {
        let entries = [("e1", "dotfiles"), ("e2", "other"), ("e3", "dotfiles")];
        assert_eq!(
            find_collisions(&entries),
            vec![("dotfiles", vec!["e1", "e3"])]
        );
    }

    #[test]
    fn find_collisions_groups_three_or_more_entries_sharing_a_name() {
        let entries = [("e1", "x"), ("e2", "x"), ("e3", "x")];
        assert_eq!(
            find_collisions(&entries),
            vec![("x", vec!["e1", "e2", "e3"])]
        );
    }

    #[test]
    fn find_collisions_orders_results_by_the_names_first_appearance() {
        let entries = [("e1", "b"), ("e2", "a"), ("e3", "b"), ("e4", "a")];
        assert_eq!(
            find_collisions(&entries),
            vec![("b", vec!["e1", "e3"]), ("a", vec!["e2", "e4"])]
        );
    }

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
