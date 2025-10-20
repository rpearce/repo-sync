use std::{io, path::Path, process};

use crate::config::Config;
use crate::git::clone::git_clone;
use crate::git::pull::git_pull;
use crate::utils::url::normalize;

/// Sync a repository at `url` into `base_dir`: clone if missing, otherwise pull updates.
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
        if let Err(e) = git_pull(&path, config) {
            eprintln!("Error pulling {}: {}", url, e)
        }
        if let Err(e) = sync_repo_branches(path.to_str().unwrap(), config) {
            eprintln!("Error syncing branches in {}: {}", url, e);
        }
    } else if let Err(e) = git_clone(&url, &path, config) {
        eprintln!("Error cloning {}: {}", url, e)
    }
}

/// Synchronize all local branches in the repository at `path` with their upstreams.
/// Current branch: fast-forward merge if working tree is clean.
/// Other branches: update directly from upstream without checkout.
/// - `path`: local repository directory
/// - `config`: command configuration
fn sync_repo_branches(path: &str, config: &Config) -> io::Result<()> {
    // Step 1: Determine the current branch name
    // `git rev-parse --abbrev-ref HEAD` returns the branch currently checked out
    let mut current_branch_output_cmd = process::Command::new("git");
    current_branch_output_cmd
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD");
    if !config.verbose {
        current_branch_output_cmd.arg("--quiet");
    }

    let current_branch_output = current_branch_output_cmd.output()?;

    let current_branch = String::from_utf8_lossy(&current_branch_output.stdout)
        .trim()
        .to_string();

    // Step 2: Fetch all remotes and prune deleted branches and tags
    // This is equivalent to `git fetch --all -Pp --quiet`
    let mut status_output_cmd = process::Command::new("git");
    status_output_cmd
        .arg("-C")
        .arg(path)
        .arg("fetch")
        .arg("--all")
        .arg("-Pp");
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
    let mut branch_pairs_output_cmd = process::Command::new("git");
    branch_pairs_output_cmd
        .arg("-C")
        .arg(path)
        .arg("for-each-ref")
        .arg("--format=%(refname:short):%(upstream:short)")
        .arg("refs/heads");
    if !config.verbose {
        branch_pairs_output_cmd.arg("--quiet");
    }

    let branch_pairs_output = branch_pairs_output_cmd.output()?;

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
            // Current branch: merge from upstream if working tree is clean

            // Check if working tree is clean using `git status --porcelain`
            let mut status_out_cmd = process::Command::new("git");
            status_out_cmd
                .arg("-C")
                .arg(path)
                .arg("status")
                .arg("--porcelain");
            if !config.verbose {
                status_out_cmd.arg("--quiet");
            }

            let status_out = status_out_cmd.output()?;

            let clean = status_out.stdout.is_empty();

            if clean {
                // Safe fast-forward merge from upstream branch
                let mut merge_cmd = process::Command::new("git");
                if !config.verbose {
                    merge_cmd.arg("--quiet");
                }

                let _ = merge_cmd
                    .arg("-C")
                    .arg(path)
                    .arg("merge")
                    .arg("--ff-only")
                    .arg(upstream)
                    .status();
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
