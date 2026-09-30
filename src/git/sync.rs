use std::{io, path::Path, process};

use crate::config::Config;
use crate::git::clone::git_clone;
use crate::utils::url::normalize;

/// Sync a repository at `url` into `base_dir`: clone if missing, otherwise
/// fetch and fast-forward it. Never runs a plain `git pull`: that would
/// honor the user's `pull.rebase` config (rewriting local commits) and
/// merge into a dirty working tree, both of which contradict a tool that
/// promises a fast-forward-only update. See `sync_repo_branches`.
/// - `url`: repository URL (partial URLs are prefixed with https://)
/// - `base_dir`: local directory for repositories
/// - `config`: command configuration
pub fn sync_repo(url: &str, config: &Config) {
    // Step 1: Normalize the URL to ensure it has a protocol (https://)
    let url = normalize(url);

    // Step 2: Determine the repository name from the URL
    // Example: "https://github.com/user/repo.git" -> "repo"
    let name = url.split("/").last().unwrap().replace(".git", "");

    // Step 3: Construct the full local path for this repository
    // Example: base_dir="/home/user/repos", name="repo" -> "/home/user/repos/repo"
    let path = Path::new(&config.output_dir).join(name);

    // Step 4: Check if the repository already exists locally
    if path.exists() {
        if let Err(e) = sync_repo_branches(path.to_str().unwrap(), config) {
            eprintln!("Error syncing branches in {}: {}", url, e);
        }
    } else if let Err(e) = git_clone(&url, &path, config) {
        eprintln!("Error cloning {}: {}", url, e)
    }
}

/// Build an `io::Error` for a failed git command that includes the
/// command and git's stderr, so a failure is never mistaken for success
/// (e.g. a failed `status --porcelain` must never be read as "clean").
/// - `command`: human-readable description of the command that failed
/// - `path`: repository directory the command ran in
/// - `stderr`: the command's captured stderr
fn git_command_failed(command: &str, path: &str, stderr: &[u8]) -> io::Error {
    io::Error::other(format!(
        "{} failed in {}: {}",
        command,
        path,
        String::from_utf8_lossy(stderr).trim()
    ))
}

/// Synchronize all local branches in the repository at `path` with their upstreams.
/// Current branch: fast-forward merge (`merge --ff-only`) if there are no
/// tracked-file modifications; untracked files never block it, since
/// `merge --ff-only` itself refuses to overwrite one that's in the way.
/// Other branches: update directly from upstream without checkout.
/// - `path`: local repository directory
/// - `config`: command configuration
fn sync_repo_branches(path: &str, config: &Config) -> io::Result<()> {
    // Step 1: Determine the current branch name
    // `git rev-parse --abbrev-ref HEAD` returns the branch currently checked out
    // Note: `--quiet` is accepted here but has no effect without
    // `--verify`, so it's just noise; omit it and check the exit status
    // directly instead.
    let current_branch_output = process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()?;

    if !current_branch_output.status.success() {
        return Err(git_command_failed(
            "git rev-parse --abbrev-ref HEAD",
            path,
            &current_branch_output.stderr,
        ));
    }

    let current_branch = String::from_utf8_lossy(&current_branch_output.stdout)
        .trim()
        .to_string();

    // Step 2: Fetch all remotes and prune deleted remote-tracking branches.
    // This is equivalent to `git fetch --all --prune --quiet`. Deliberately
    // not `-Pp`/`--prune-tags`: that also deletes local tags the remote
    // doesn't have, including ones that were never pushed.
    let mut status_output_cmd = process::Command::new("git");
    status_output_cmd
        .arg("-C")
        .arg(path)
        .arg("fetch")
        .arg("--all")
        .arg("--prune");
    if !config.verbose {
        status_output_cmd.arg("--quiet");
    }

    let status_output = status_output_cmd.status()?;

    if !status_output.success() {
        return Err(std::io::Error::other(format!(
            "git fetch --all failed in {}",
            path
        )));
    }

    // Step 3: List all local branches and their upstream branches
    // Format: "<local-branch>:<upstream-branch>"
    // Note: `--quiet` is not a valid flag for `for-each-ref`; passing it
    // made this command exit non-zero, so `branch_pairs` was always empty
    // and no non-current branch was ever updated without `-v`.
    let branch_pairs_output = process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("for-each-ref")
        .arg("--format=%(refname:short):%(upstream:short)")
        .arg("refs/heads")
        .output()?;

    if !branch_pairs_output.status.success() {
        return Err(git_command_failed(
            "git for-each-ref refs/heads",
            path,
            &branch_pairs_output.stderr,
        ));
    }

    let branch_pairs = String::from_utf8_lossy(&branch_pairs_output.stdout);

    // Step 4: Iterate over each local:upstream pair
    for line in branch_pairs.lines() {
        // Skip branches that have no upstream (they end with ':')
        if line.ends_with(":") {
            continue;
        }

        // Split into local and upstream branch names
        let mut parts = line.splitn(2, ':');
        let local = parts.next().unwrap().trim();
        let upstream = parts.next().unwrap().trim();

        if local == current_branch {
            // Current branch: merge from upstream if there are no tracked
            // modifications.

            // Check for tracked modifications using
            // `git status --porcelain --untracked-files=no`. Untracked
            // files are deliberately excluded: counting them as "dirty"
            // would block fast-forwards that work fine today, and
            // `merge --ff-only` itself refuses to overwrite an untracked
            // file that's actually in the way.
            // Note: `--quiet` is not a valid flag for `status` here; omit
            // it and check the exit status directly. Fail closed: if the
            // status check itself fails, we must not treat that as
            // "clean" and merge anyway.
            let status_out = process::Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("status")
                .arg("--porcelain")
                .arg("--untracked-files=no")
                .output()?;

            if !status_out.status.success() {
                return Err(git_command_failed(
                    "git status --porcelain --untracked-files=no",
                    path,
                    &status_out.stderr,
                ));
            }

            let clean = status_out.stdout.is_empty();

            if clean {
                // Safe fast-forward merge from upstream branch.
                // `--quiet` goes after the `merge` subcommand: it is not
                // a valid top-level git option.
                let mut merge_cmd = process::Command::new("git");
                merge_cmd.arg("-C").arg(path).arg("merge").arg("--ff-only");
                if !config.verbose {
                    merge_cmd.arg("--quiet");
                }
                merge_cmd.arg(upstream);

                // Non-fatal: merge failures are reported by inherited
                // stderr, not turned into a returned error (unchanged
                // from before; out of scope for this fix).
                let _ = merge_cmd.status();
            } else if config.verbose {
                // Working tree dirty: skip merge to avoid conflicts
                println!("Skipping merge on {} (dirty branch)", local);
            }
        } else {
            // Non-current branch: update directly from upstream without checkout
            // Equivalent to: `git fetch . <upstream>:<local>`
            let mut fetch_cmd = process::Command::new("git");
            fetch_cmd
                .arg("-C")
                .arg(path)
                .arg("fetch")
                .arg(".")
                .arg(format!("{}:{}", upstream, local));
            if !config.verbose {
                fetch_cmd.arg("--quiet");
            }

            let _ = fetch_cmd.status();
        }
    }

    Ok(())
}
