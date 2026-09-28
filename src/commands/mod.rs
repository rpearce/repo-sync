use std::{fs, process::ExitCode};

use crate::config::Config;
use crate::error::RepoError;

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

/// Print each failure, then the run's summary line, and return the
/// resulting process exit code. Shared by `clone::run` and `sync::run`
/// (and by later tasks' pre-`par_iter` failures, e.g. a directory-name
/// collision, which feed into the same `results` slice) so the summary
/// format and its stdout/stderr routing live in exactly one place.
///
/// Per the project's summary rules: success stays silent unless
/// `verbose` is set (then the summary goes to stdout); any failure
/// always prints the summary, to stderr, regardless of `verbose`.
/// - `verb`: past-tense verb for the summary, e.g. `"Cloned"` or `"Synced"`
/// - `results`: one result per repository processed
/// - `verbose`: whether `-v` was passed
pub(crate) fn report(verb: &str, results: &[Result<(), RepoError>], verbose: bool) -> ExitCode {
    for result in results {
        if let Err(e) = result {
            eprintln!("{e}");
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
