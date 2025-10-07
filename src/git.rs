use std::{fs, path, process};

use crate::config::Config;
use crate::error::{RepoSyncError, Result};
use crate::utils::path::extract_repo_name;
use crate::utils::output::RepoStatusPrinter;

/// Clone a Git repository from `url` into `path`.
/// - `url`: pre-validated repository URL
/// - `path`: local target directory
pub fn git_clone(url: &str, path: &str) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path::Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|e| {
            RepoSyncError::directory(
                parent.to_string_lossy(),
                format!("Failed to create parent directory: {}", e)
            )
        })?;
    }

    // Spawn a `git clone <url> <path>` process and wait for it to finish
    let output = process::Command::new("git")
        .arg("clone")
        .arg(url)
        .arg(path)
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(RepoSyncError::git(
            "clone",
            format!("Failed to clone {}: {}", url, stderr.trim())
        ))
    }
}

/// Pull updates in the Git repository at `path`.
/// - `path`: local repository directory
pub fn git_pull(path: &str) -> Result<()> {
    // Verify path exists and is a directory
    if !path::Path::new(path).is_dir() {
        return Err(RepoSyncError::directory(path, "Path does not exist or is not a directory"));
    }

    // Spawn a `git -C <path> pull` process to pull the latest changes
    // `-C <path>` tells Git to operate in the specified directory
    let output = process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("pull")
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(RepoSyncError::git(
            "pull",
            format!("Failed to pull in {}: {}", path, stderr.trim())
        ))
    }
}

/// Clone a repository only if it doesn't already exist.
/// - `url`: pre-validated repository URL
/// - `base_dir`: directory where the repo should be cloned
/// - `config`: configuration including verbose mode
pub fn clone_repo(url: &str, base_dir: &str, config: &Config) -> Result<()> {
    let name = extract_repo_name(url)?;
    let path = path::Path::new(base_dir).join(&name);
    let printer = RepoStatusPrinter::new(config);

    if path.exists() {
        printer.skipped(&name, "already exists");
        Ok(())
    } else {
        let path_str = path.to_str().ok_or_else(|| {
            RepoSyncError::config(format!(
                "Repository path contains invalid UTF-8: {}",
                path.display()
            ))
        })?;
        git_clone(url, path_str)?;
        printer.cloned(&name);
        Ok(())
    }
}

/// Sync a repository at `url` into `base_dir`: clone if missing, otherwise pull updates.
/// - `url`: pre-validated repository URL
/// - `base_dir`: local directory for repositories
/// - `config`: configuration including verbose mode
pub fn sync_repo(url: &str, base_dir: &str, config: &Config) -> Result<()> {
    // Step 1: Determine the repository name from the URL
    let name = extract_repo_name(url)?;
    let printer = RepoStatusPrinter::new(config);

    // Step 2: Construct the full local path for this repository
    let path = path::Path::new(base_dir).join(&name);

    // Step 3: Check if the repository already exists locally
    if path.exists() {
        let path_str = path.to_str().ok_or_else(|| {
            RepoSyncError::config(format!(
                "Repository path contains invalid UTF-8: {}",
                path.display()
            ))
        })?;
        git_pull(path_str)?;
        sync_repo_branches(path_str, config)?;
        printer.synced(&name);
    } else {
        let path_str = path.to_str().ok_or_else(|| {
            RepoSyncError::config(format!(
                "Repository path contains invalid UTF-8: {}",
                path.display()
            ))
        })?;
        git_clone(url, path_str)?;
        printer.cloned(&name);
    }
    Ok(())
}

/// Synchronize all local branches in the repository at `path` with their upstreams.
/// - Current branch: fast-forward merge if working tree is clean
/// - Other branches: update directly from upstream without checkout
pub fn sync_repo_branches(path: &str, config: &Config) -> Result<()> {
    // Step 1: Determine the current branch name
    // `git rev-parse --abbrev-ref HEAD` returns the branch currently checked out
    let current_branch_output = process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()?;

    let current_branch = String::from_utf8_lossy(&current_branch_output.stdout)
        .trim()
        .to_string();

    // Step 2: Fetch all remotes and prune deleted branches and tags
    // This is equivalent to `git fetch --all -Pp --quiet`
    let status_output = process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("fetch")
        .arg("--all")
        .arg("-Pp")
        .arg("--quiet")
        .status()?;

    if !status_output.success() {
        return Err(RepoSyncError::git(
            "fetch",
            format!("git fetch --all failed in {}", path)
        ));
    }

    // Step 3: List all local branches and their upstream branches
    // Format: "<local-branch>:<upstream-branch>"
    let branch_pairs_output = process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("for-each-ref")
        .arg("--format=%(refname:short):%(upstream:short)")
        .arg("refs/heads")
        .output()?;

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
            let status_out = process::Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("status")
                .arg("--porcelain")
                .output()?;

            let clean = status_out.stdout.is_empty();

            if clean {
                // Safe fast-forward merge from upstream branch
                let merge_result = process::Command::new("git")
                    .arg("-C")
                    .arg(path)
                    .arg("merge")
                    .arg("--ff-only")
                    .arg(upstream)
                    .status();

                if let Err(e) = merge_result {
                    eprintln!("Warning: Failed to merge {} from {}: {}", local, upstream, e);
                } else if let Ok(status) = merge_result {
                    if !status.success() {
                        eprintln!("Warning: Git merge failed for branch {} from {}", local, upstream);
                    }
                }
            } else {
                // Working tree dirty: skip merge to avoid conflicts
                let printer = RepoStatusPrinter::new(config);
                printer.skip_merge(local, "dirty branch");
            }
        } else {
            // Non-current branch: update directly from upstream without checkout
            // Equivalent to: `git fetch . <upstream>:<local>`
            let fetch_result = process::Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("fetch")
                .arg(".")
                .arg(format!("{}:{}", upstream, local))
                .status();

            if let Err(e) = fetch_result {
                eprintln!("Warning: Failed to update branch {} from {}: {}", local, upstream, e);
            } else if let Ok(status) = fetch_result {
                if !status.success() {
                    eprintln!("Warning: Git fetch failed for branch {} from {}", local, upstream);
                }
            }
        }
    }

    Ok(())
}
