use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
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
/// Two passes happen before the parallel phase, both on each entry's
/// **normalized** URL (see `crate::utils::url::normalize`) rather than
/// its raw repo-list text, matching the convention `RepoError::Clone`,
/// `RepoError::Sync` and `RepoError::NoDirectoryName` already follow for
/// their own messages:
///
/// 1. **Exact duplicates.** Two entries that normalize to the identical
///    URL aren't a collision between two different repositories — they
///    name the same one twice (e.g. one bare `host/owner/repo` line and
///    an `http://` line that upgrades to a URL the other side already
///    uses). The first occurrence is kept; every repeat is dropped and
///    reported with a warning to stderr (a warning, so it never changes
///    the exit code), and doesn't count toward the run's totals at all.
/// 2. **Directory-name collisions.** Among what's left, two or more
///    *different* normalized URLs that derive the same directory name
///    (see `crate::utils::url::repo_name`) would otherwise race to
///    clone/sync into the same path, with the winner decided
///    nondeterministically by whichever one gets there first (see expert
///    evidence B6). Checking this up front makes the outcome
///    deterministic instead: every entry sharing a contested name is
///    folded into one failure and none of them run, while every other
///    entry still does.
///
/// An entry with no usable directory name at all (`repo_name` returns
/// `None`) doesn't participate in collision detection here — that's
/// already a per-entry failure raised inside `clone_repo`/`sync_repo`, so
/// it's included in `to_run` and left for them to reject individually.
///
/// Returns `(to_run, failures)`:
/// - `to_run`: every surviving entry that should still go to the
///   parallel phase, in original order, as its original repo-list text
///   (not normalized — `clone_repo`/`sync_repo` normalize it themselves);
/// - `failures`: one `RepoError::DuplicateName` **per collision**, not
///   per colliding entry — see `RepoError::failed_entries`, which is how
///   `report` still counts every colliding entry once toward the failed
///   total even though there's only one `Err` to print.
/// - `entries`: raw repo-list entries, e.g. from `parse_repo_list`
pub(crate) fn partition_collisions(entries: &[&str]) -> (Vec<String>, Vec<RepoError>) {
    let normalized: Vec<String> = entries.iter().map(|entry| normalize(entry)).collect();

    // Keep the first occurrence of each normalized URL; warn about (and
    // drop) every repeat before it can reach collision detection or the
    // parallel phase.
    let mut seen: HashSet<&str> = HashSet::new();
    let mut unique_indices: Vec<usize> = Vec::new();
    for (i, norm) in normalized.iter().enumerate() {
        if seen.insert(norm.as_str()) {
            unique_indices.push(i);
        } else {
            eprintln!("warning: duplicate entry '{norm}' ignored");
        }
    }

    // (normalized URL, derived name) pairs, one per surviving entry that
    // has a usable name.
    let named: Vec<(&str, &str)> = unique_indices
        .iter()
        .filter_map(|&i| repo_name(&normalized[i]).map(|name| (normalized[i].as_str(), name)))
        .collect();

    // Maps a surviving entry's normalized URL back to its index, so a
    // collision (reported by `find_collisions` as normalized-URL
    // strings) can be traced back to the specific entries to exclude
    // from `to_run`. Unambiguous because exact duplicates were already
    // removed above, so every surviving normalized URL is unique.
    let index_by_norm: HashMap<&str, usize> = unique_indices
        .iter()
        .map(|&i| (normalized[i].as_str(), i))
        .collect();

    let mut colliding: HashSet<usize> = HashSet::new();
    let mut failures = Vec::new();

    for (name, group) in find_collisions(&named) {
        for &norm_url in &group {
            if let Some(&i) = index_by_norm.get(norm_url) {
                colliding.insert(i);
            }
        }
        failures.push(RepoError::DuplicateName {
            name: name.to_string(),
            entries: group
                .into_iter()
                .map(|norm_url| norm_url.to_string())
                .collect(),
        });
    }

    let to_run: Vec<String> = unique_indices
        .into_iter()
        .filter(|i| !colliding.contains(i))
        .map(|i| entries[i].to_string())
        .collect();

    (to_run, failures)
}

/// How many repo-list entries a run's verbose header line should report,
/// given `partition_collisions`'s output. This is *not* the raw parsed
/// entry count: `partition_collisions` drops exact-duplicate entries
/// before returning, so counting the raw list would still include a
/// dropped duplicate (e.g. printing "Cloning 2 repositories" for a list
/// with one URL repeated, right above a summary that correctly says
/// "Cloned 1 repositories"). Instead this sums `to_run` with the
/// weighted failures `partition_collisions` already computed
/// (`RepoError::failed_entries`, which is how a `DuplicateName` failure
/// standing in for several colliding entries counts as more than one) —
/// the same total `report`'s summary line uses, so the header and the
/// summary always agree. Shared by `clone::run` and `sync::run` so this
/// arithmetic lives in exactly one place.
/// - `to_run`: survivors returned by `partition_collisions`
/// - `failures`: precomputed failures returned by `partition_collisions`
pub(crate) fn unique_entry_count(to_run: &[String], failures: &[RepoError]) -> usize {
    to_run.len()
        + failures
            .iter()
            .map(RepoError::failed_entries)
            .sum::<usize>()
}

/// Print each failure, then the run's summary line, and return the
/// resulting process exit code. Shared by `clone::run` and `sync::run`
/// (and, through them, by `partition_collisions`'s pre-`par_iter`
/// failures, e.g. a directory-name collision, which feed into the same
/// `results` slice) so the summary format and its stdout/stderr routing
/// live in exactly one place.
///
/// `failed` (and so `total`) sums each error's `RepoError::failed_entries`
/// weight rather than just counting `Err` results: `partition_collisions`
/// emits one `RepoError::DuplicateName` per *collision*, standing in for
/// every entry named in it, so that one `Err` is weighted by how many
/// entries it represents. This is also why printing every `Err` here
/// (one `eprintln!` each, no de-duplication) still prints a collision's
/// message exactly once: there's only one `Err` for it to begin with.
///
/// Per the project's summary rules: success stays silent unless
/// `verbose` is set (then the summary goes to stdout); any failure
/// always prints the summary, to stderr, regardless of `verbose`.
/// - `verb`: past-tense verb for the summary, e.g. `"Cloned"` or `"Synced"`
/// - `results`: one result per repository processed, except a
///   `DuplicateName` result, which represents every entry it names
/// - `verbose`: whether `-v` was passed
pub(crate) fn report(verb: &str, results: &[Result<(), RepoError>], verbose: bool) -> ExitCode {
    for result in results {
        if let Err(e) = result {
            eprintln!("{e}");
        }
    }

    let failed: usize = results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .map(RepoError::failed_entries)
        .sum();
    let ok = results.iter().filter(|r| r.is_ok()).count();
    let total = ok + failed;
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

/// Run `f` — a parallel workload, typically a
/// `to_run.par_iter().map(...).collect()` over one command's surviving
/// repo-list entries — bounded to `jobs` threads when set (`-j`/
/// `--jobs`), or under rayon's own default global pool otherwise (which
/// already honors `RAYON_NUM_THREADS` and otherwise defaults to the
/// number of CPUs). Shared by `clone::run` and `sync::run` so this "which
/// pool does the parallel phase run in" decision lives in exactly one
/// place.
///
/// When `jobs` is `Some`, builds a **local** pool with
/// `rayon::ThreadPoolBuilder::build` and runs `f` inside it via
/// `ThreadPool::install`, deliberately never `build_global`: the global
/// pool can only be initialized once per process and errors on a second
/// attempt, which would break any caller that runs more than one
/// `clone`/`sync` in the same process (e.g. tests). A failure to build
/// the local pool is reported to stderr and treated as a run failure,
/// exactly like any other `ExitCode::FAILURE`.
/// - `jobs`: `-j`/`--jobs` value; `None` means "use rayon's default"
/// - `f`: the parallel workload to run
pub(crate) fn run_in_pool<F, R>(jobs: Option<NonZeroUsize>, f: F) -> Result<R, ExitCode>
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    let Some(n) = jobs else {
        return Ok(f());
    };

    match rayon::ThreadPoolBuilder::new().num_threads(n.get()).build() {
        Ok(pool) => Ok(pool.install(f)),
        Err(e) => {
            eprintln!("error: could not build a thread pool with {n} jobs: {e}");
            Err(ExitCode::FAILURE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partition_collisions_emits_one_duplicate_name_error_per_collision() {
        let entries = [
            "https://host/a/dotfiles",
            "https://host/b/dotfiles",
            "https://host/other",
        ];

        let (to_run, failures) = partition_collisions(&entries);

        assert_eq!(to_run, vec!["https://host/other".to_string()]);
        assert_eq!(
            failures.len(),
            1,
            "one collision must be one error, got {failures:?}"
        );
        match &failures[0] {
            RepoError::DuplicateName { name, entries } => {
                assert_eq!(name, "dotfiles");
                assert_eq!(
                    entries,
                    &vec![
                        "https://host/a/dotfiles".to_string(),
                        "https://host/b/dotfiles".to_string(),
                    ]
                );
            }
            other => panic!("expected DuplicateName, got {other:?}"),
        }
    }

    #[test]
    fn partition_collisions_names_normalized_urls_even_for_a_bare_entry() {
        let entries = ["host/a/dotfiles", "http://host/b/dotfiles"];

        let (_, failures) = partition_collisions(&entries);

        assert_eq!(failures.len(), 1);
        match &failures[0] {
            RepoError::DuplicateName { entries, .. } => {
                assert_eq!(
                    entries,
                    &vec![
                        "https://host/a/dotfiles".to_string(),
                        "https://host/b/dotfiles".to_string(),
                    ],
                    "message must show normalized URLs, not the raw list lines"
                );
            }
            other => panic!("expected DuplicateName, got {other:?}"),
        }
    }

    #[test]
    fn partition_collisions_drops_exact_duplicate_normalized_urls_without_a_collision() {
        let entries = ["https://host/a/dotfiles", "https://host/a/dotfiles"];

        let (to_run, failures) = partition_collisions(&entries);

        assert_eq!(to_run, vec!["https://host/a/dotfiles".to_string()]);
        assert!(
            failures.is_empty(),
            "an exact duplicate must not be reported as a collision, got {failures:?}"
        );
    }

    #[test]
    fn partition_collisions_still_flags_different_urls_sharing_a_name() {
        let entries = ["https://host/u/r", "https://host/u/r.git"];

        let (to_run, failures) = partition_collisions(&entries);

        assert!(to_run.is_empty());
        assert_eq!(
            failures.len(),
            1,
            "two different URLs sharing a name must still collide, got {failures:?}"
        );
    }
}
