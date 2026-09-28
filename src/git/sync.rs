use std::{io, path::Path};

use crate::config::Config;
use crate::error::RepoError;
use crate::git::clone::git_clone;
use crate::git::command;
use crate::utils::url::{normalize, repo_name};

/// Sync a repository at `url` into `base_dir`: clone if missing, otherwise
/// fetch and fast-forward it. Never runs a plain `git pull`: that would
/// honor the user's `pull.rebase` config (rewriting local commits) and
/// merge into a dirty working tree, both of which contradict a tool that
/// promises a fast-forward-only update. See `sync_repo_branches`.
/// - `url`: repository URL (partial URLs are prefixed with https://)
/// - `config`: command configuration
pub fn sync_repo(url: &str, config: &Config) -> Result<(), RepoError> {
    // Step 1: Normalize the URL (e.g. upgrade http:// to https://, or
    // prefix a bare host/owner/repo with https://). ssh/scp-style/local-path
    // entries are left as-is; see `normalize` for the exact rules.
    let url = normalize(url);

    // Step 2: Determine the repository name from the URL
    // Example: "https://github.com/user/repo.git" -> "repo"
    let name = repo_name(&url).ok_or_else(|| RepoError::NoDirectoryName { entry: url.clone() })?;

    // Step 3: Construct the full local path for this repository
    // Example: base_dir="/home/user/repos", name="repo" -> "/home/user/repos/repo"
    let path = Path::new(&config.output_dir).join(name);

    // Step 4: Check if the repository already exists locally. A plain
    // `path.exists()` isn't enough: git's own repository discovery walks
    // up from `-C path` to the nearest enclosing `.git` if `path` itself
    // isn't a repo, so running git there would silently operate on
    // whatever repository happens to enclose `path` instead. `.git` may
    // be a file rather than a directory (worktrees, submodules), which
    // `exists()` covers either way.
    if path.exists() {
        if path.join(".git").exists() {
            sync_repo_branches(&path, config).map_err(|source| RepoError::Sync { url, source })
        } else {
            Err(RepoError::NotAGitRepository { path })
        }
    } else {
        git_clone(&url, &path, config).map_err(|source| RepoError::Clone { url, source })
    }
}

/// Print a single attributed `warning:` line for a non-fatal branch-level
/// update failure (a non-fast-forward `merge`, or a `fetch .` rejecting a
/// diverged non-current branch), instead of leaving it silently ignored
/// or letting git's own multi-line, unattributed stderr reach the user
/// directly. Per the error taxonomy, a branch-level update failure is a
/// warning: it never changes the process exit code.
///
/// Uses the **last** non-empty stderr line, not the first: `fetch .`'s
/// rejection puts its actually-useful explanation
/// (`! [rejected] ... (non-fast-forward)`) after an uninformative `From .`
/// header line, and a diverged `merge --ff-only` puts its
/// `fatal: Not possible to fast-forward, aborting.` last too (after any
/// hint lines, though `-c advice.diverging=false` already suppresses
/// those — see the merge call in `sync_repo_branches`).
/// - `repo`: identifies which repository this warning is about (the
///   repo's directory name, since that's what's on hand in
///   `sync_repo_branches`; the normalized URL isn't plumbed down this far)
/// - `branch`: the local branch that couldn't be updated
/// - `err`: the failed git command's error, whose message is git's
///   captured stderr (see `command::git_output`/`command::git_output_combined`)
fn warn_branch_update_failed(repo: &str, branch: &str, err: &io::Error) {
    let message = err.to_string();
    let last_line = message
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");
    eprintln!("warning: {repo}: could not fast-forward {branch}: {last_line}");
}

/// Print each non-blank line of `output` — git's own stdout/stderr from a
/// *successful* branch-level update (see `command::git_output_combined`)
/// — prefixed with `<repo>: `, so verbose mode still shows git's own
/// progress text (e.g. `fetch .`'s `From . ... -> feature`, or
/// `merge --ff-only`'s `Updating a..b` / `Fast-forward`), now attributed
/// to a specific repo instead of raw, unattributed inherited stdio (or,
/// since these commands are captured rather than inherited, silently
/// dropped entirely).
/// - `repo`: identifies which repository this output is about (see
///   `warn_branch_update_failed`)
/// - `output`: the successful command's combined stdout/stderr
fn print_branch_update_output(repo: &str, output: &str) {
    for line in output.lines() {
        if !line.trim().is_empty() {
            println!("{repo}: {line}");
        }
    }
}

/// Synchronize all local branches in the repository at `path` with their upstreams.
/// Current branch: fast-forward merge (`merge --ff-only`) if there are no
/// tracked-file modifications; untracked files never block it, since
/// `merge --ff-only` itself refuses to overwrite one that's in the way.
/// Other branches: update directly from upstream without checkout. A
/// diverged/non-fast-forwardable branch (current or not) is reported as
/// one attributed warning (see `warn_branch_update_failed`) rather than
/// silently ignored or left as git's own unattributed stderr noise.
/// - `path`: local repository directory
/// - `config`: command configuration
fn sync_repo_branches(path: &Path, config: &Config) -> io::Result<()> {
    // Identifies this repo in a branch-level warning: the normalized URL
    // isn't available this far down, but the repo's directory name is,
    // and it's the same name `repo_name` derived the URL down to in
    // `sync_repo`.
    let repo = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("<unknown>");

    // Step 1: Determine HEAD's full symbolic ref, if it has one.
    // `git symbolic-ref -q HEAD` resolves HEAD's symbolic target without
    // requiring it to already point at a commit, unlike `rev-parse
    // --abbrev-ref HEAD` (which fails on an "unborn" HEAD — e.g. right
    // after cloning a remote with no commits yet, where HEAD still
    // points at `refs/heads/main` but that ref doesn't exist yet). `-q`
    // also makes it fail quietly (no stderr) on a detached HEAD, which
    // correctly means "no current branch": every local branch is then
    // updated the same way as a non-current one, via `fetch .` below.
    let current_branch_ref = command::git_output(path, &["symbolic-ref", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string());

    // Step 2: Fetch all remotes and prune deleted remote-tracking branches.
    // This is equivalent to `git fetch --all --prune --quiet`. Deliberately
    // not `-Pp`/`--prune-tags`: that also deletes local tags the remote
    // doesn't have, including ones that were never pushed.
    command::git_run(path, &["fetch", "--all", "--prune"], config.verbose)
        .map_err(|_| io::Error::other(format!("git fetch --all failed in {}", path.display())))?;

    // Step 3: List all local branches together with their upstream's full
    // refname and tracking status. Full refnames (not
    // `%(refname:short)`/`%(upstream:short)`) are used defensively, so
    // correctness here doesn't depend on git's own short-name
    // disambiguation (which decides what a short name like `origin/main`
    // resolves to when a same-named local tag or other ref could also
    // match) continuing to pick the actual upstream in `merge --ff-only` /
    // `fetch .` below. `%(upstream:track)` reports `[gone]` when the
    // upstream's remote-tracking ref no longer exists because its branch
    // was deleted on the remote. Fields are NUL (`%00`) separated so a
    // `/`-containing refname can never be confused with a `:`-separated
    // delimiter.
    let branch_pairs = command::git_output(
        path,
        &[
            "for-each-ref",
            "--format=%(refname)%00%(upstream)%00%(upstream:track)",
            "refs/heads",
        ],
    )
    .map_err(|e| command::command_failed("git for-each-ref refs/heads", path, e))?;

    // Step 4: Iterate over each local branch and its upstream.
    for line in branch_pairs.lines() {
        let mut fields = line.splitn(3, '\0');
        let local_ref = fields.next().unwrap_or("").trim();
        let upstream_ref = fields.next().unwrap_or("").trim();
        let track = fields.next().unwrap_or("").trim();

        // Branch name for messages, e.g. "feature" instead of
        // "refs/heads/feature".
        let local = local_ref.strip_prefix("refs/heads/").unwrap_or(local_ref);

        if upstream_ref.is_empty() {
            // No upstream configured at all: nothing to update from.
            continue;
        }

        if track == "[gone]" {
            // The upstream's remote-tracking ref no longer exists (its
            // branch was deleted on the remote). Fetching or merging
            // from it would fail with the same git error on every single
            // run, for a reason the user can't fix by re-running sync,
            // so this isn't a failure or even a one-off warning: just
            // skip it, and only mention it at all in verbose mode.
            if config.verbose {
                println!("Skipping {local} (upstream gone)");
            }
            continue;
        }

        if Some(local_ref) == current_branch_ref.as_deref() {
            // Current branch: merge from upstream if there are no tracked
            // modifications.

            // Check for tracked modifications using
            // `git status --porcelain --untracked-files=no`. Untracked
            // files are deliberately excluded: counting them as "dirty"
            // would block fast-forwards that work fine today, and
            // `merge --ff-only` itself refuses to overwrite an untracked
            // file that's actually in the way.
            // Fail closed: if the status check itself fails, we must not
            // treat that as "clean" and merge anyway.
            let status_out =
                command::git_output(path, &["status", "--porcelain", "--untracked-files=no"])
                    .map_err(|e| {
                        command::command_failed(
                            "git status --porcelain --untracked-files=no",
                            path,
                            e,
                        )
                    })?;

            let clean = status_out.is_empty();

            if clean {
                // Safe fast-forward merge from upstream branch. Captured
                // (not inherited) so a failure can be attributed to this
                // repo and branch in one line instead of raw,
                // unattributed git noise. `-c advice.diverging=false`
                // suppresses ~10 lines of generic hint text about a
                // diverged branch that this warning already explains.
                // Non-fatal: a failed fast-forward is a warning, not a
                // failure (see the error taxonomy). On success, verbose
                // mode still shows git's own output (e.g. `Updating a..b`
                // / `Fast-forward`), attributed to this repo, instead of
                // capturing it only to throw it away.
                match command::git_output_combined(
                    path,
                    &[
                        "-c",
                        "advice.diverging=false",
                        "merge",
                        "--ff-only",
                        upstream_ref,
                    ],
                ) {
                    Ok(output) => {
                        if config.verbose {
                            print_branch_update_output(repo, &output);
                        }
                    }
                    Err(e) => warn_branch_update_failed(repo, local, &e),
                }
            } else if config.verbose {
                // Working tree dirty: skip merge to avoid conflicts
                println!("Skipping merge on {} (dirty branch)", local);
            }
        } else {
            // Non-current branch: update directly from upstream without
            // checkout. Equivalent to: `git fetch . <upstream-full>:<local-full>`.
            // Captured (not inherited), for the same reason as the merge
            // above: a non-fast-forward here (e.g. a diverged branch) is
            // a warning, not a failure (a `[gone]` upstream never reaches
            // this point at all; it's skipped above). `fetch` writes its
            // progress (`From . ... -> feature`) to stderr even on
            // success, so verbose mode needs the combined output to show
            // it, attributed to this repo.
            let refspec = format!("{upstream_ref}:{local_ref}");
            match command::git_output_combined(path, &["fetch", ".", &refspec]) {
                Ok(output) => {
                    if config.verbose {
                        print_branch_update_output(repo, &output);
                    }
                }
                Err(e) => warn_branch_update_failed(repo, local, &e),
            }
        }
    }

    Ok(())
}
