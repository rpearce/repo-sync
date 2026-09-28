use std::collections::HashSet;
use std::{fs, process::ExitCode};

use crate::config::Config;
use crate::error::RepoError;
use crate::utils::repo_list::find_collisions;
use crate::utils::url::{normalize, repo_name};

pub mod clone;
pub mod sync;

/// Read a command's repo-list file, reporting a clean error (rather than
/// panicking) if it can't be read.
/// - `config`: command configuration; the repo-list path is `config.repos_file`
pub(crate) fn read_repo_list(config: &Config) -> Result<String, ExitCode> {
    fs::read_to_string(&config.repos_file).map_err(|e| {
        eprintln!(
            "error: cannot read repo list '{}': {e}",
            config.repos_file.display()
        );
        ExitCode::FAILURE
    })
}

/// Split repo-list entries into those safe to hand to the parallel
/// clone/sync phase and precomputed failures for the ones that aren't.
/// Shared by `clone::run` and `sync::run` so this logic lives in exactly
/// one place.
///
/// Two or more entries that derive the same directory name (see
/// `crate::utils::url::repo_name`) would otherwise race to clone/sync
/// into the same path, with the winner decided nondeterministically by
/// whichever one gets there first (see expert evidence B6). Checking this
/// up front, before the parallel phase, makes the outcome deterministic
/// instead: every entry sharing a contested name is reported as a
/// failure and none of them run, while every other entry still does.
///
/// An entry with no usable directory name at all (`repo_name` returns
/// `None`) doesn't participate in collision detection here — that's
/// already a per-entry failure raised inside `clone_repo`/`sync_repo`, so
/// it's included in `to_run` and left for them to reject individually.
///
/// Returns `(to_run, failures)`:
/// - `to_run`: every entry that should still go to the parallel phase, in
///   original order, minus any colliding entries;
/// - `failures`: one `RepoError::DuplicateName` per colliding entry (so
///   each one counts once toward `report`'s failed total), naming, in
///   its message, every entry that shares that name, as normalized URLs.
///   All entries in one collision get the identical message; `report`
///   prints each distinct message only once (see its doc comment).
/// - `entries`: raw repo-list entries, e.g. from `parse_repo_list`
pub(crate) fn partition_collisions(entries: &[&str]) -> (Vec<String>, Vec<RepoError>) {
    let normalized: Vec<String> = entries.iter().map(|entry| normalize(entry)).collect();

    let named: Vec<(&str, &str)> = entries
        .iter()
        .copied()
        .zip(&normalized)
        .filter_map(|(entry, norm)| repo_name(norm).map(|name| (entry, name)))
        .collect();

    let mut colliding: HashSet<&str> = HashSet::new();
    let mut failures = Vec::new();

    for (name, group) in find_collisions(&named) {
        let entries_display: Vec<String> = group.iter().map(|entry| entry.to_string()).collect();
        for entry in group {
            colliding.insert(entry);
            failures.push(RepoError::DuplicateName {
                name: name.to_string(),
                entries: entries_display.clone(),
            });
        }
    }

    let to_run: Vec<String> = entries
        .iter()
        .copied()
        .filter(|entry| !colliding.contains(*entry))
        .map(|entry| entry.to_string())
        .collect();

    (to_run, failures)
}

/// Print each distinct failure message once, then the run's summary
/// line, and return the resulting process exit code. Shared by
/// `clone::run` and `sync::run` (and, through them, by
/// `partition_collisions`'s pre-`par_iter` failures, e.g. a
/// directory-name collision, which feed into the same `results` slice)
/// so the summary format and its stdout/stderr routing live in exactly
/// one place.
///
/// Deduplicating by message text (rather than printing every `Err`) is
/// what keeps a single collision from printing its
/// `'<name>' is used by multiple entries: ...` message once per colliding
/// entry: `partition_collisions` gives every entry in one collision the
/// identical `RepoError::DuplicateName`, and each of those still counts
/// once toward `failed` below since `results` still holds one `Err` per
/// entry.
///
/// Per the project's summary rules: success stays silent unless
/// `verbose` is set (then the summary goes to stdout); any failure
/// always prints the summary, to stderr, regardless of `verbose`.
/// - `verb`: past-tense verb for the summary, e.g. `"Cloned"` or `"Synced"`
/// - `results`: one result per repository processed
/// - `verbose`: whether `-v` was passed
pub(crate) fn report(verb: &str, results: &[Result<(), RepoError>], verbose: bool) -> ExitCode {
    let mut printed: HashSet<String> = HashSet::new();
    for result in results {
        if let Err(e) = result {
            let message = e.to_string();
            if printed.insert(message.clone()) {
                eprintln!("{message}");
            }
        }
    }

    let total = results.len();
    let failed = results.iter().filter(|r| r.is_err()).count();
    let ok = total - failed;
    let summary = format!("{verb} {total} repositories: {ok} ok, {failed} failed");

    if failed > 0 {
        eprintln!("{summary}");
        return ExitCode::FAILURE;
    }

    if verbose {
        println!("{summary}");
    }

    ExitCode::SUCCESS
}
