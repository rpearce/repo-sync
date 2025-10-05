use std::{io, path, process};

/// Clone a Git repository from `url` into `path`.
/// - `url`: repository URL
/// - `path`: local target directory
pub fn git_clone(url: &str, path: &str) -> io::Result<()> {
    // Spawn a `git clone <url> <path>` process and wait for it to finish
    let status = process::Command::new("git")
        .arg("clone")
        .arg(url)
        .arg(path)
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("git clone failed for {}", url),
        ))
    }
}

/// Pull updates in the Git repository at `path`.
/// - `path`: local repository directory
pub fn git_pull(path: &str) -> io::Result<()> {
    // Spawn a `git -C <path> pull` process to pull the latest changes
    // `-C <path>` tells Git to operate in the specified directory
    let status = process::Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("pull")
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("git pull failed in {}", path),
        ))
    }
}

/// Clone a repository only if it doesn't already exist.
/// - `url`: repository URL
/// - `base_dir`: directory where the repo should be cloned
pub fn clone_repo(url: &str, base_dir: &str) {
    let url = normalize_url(url);
    let name = url.split('/').last().unwrap().replace(".git", "");
    let path = path::Path::new(base_dir).join(&name);

    if path.exists() {
        println!("Skipping {}, already exists", name);
    } else {
        if let Err(e) = git_clone(&url, path.to_str().unwrap()) {
            eprintln!("Error cloning {}: {}", url, e);
        }
    }
}

/// Sync a repository at `url` into `base_dir`: clone if missing, otherwise pull updates.
/// - `url`: repository URL (partial URLs are prefixed with https://)
/// - `base_dir`: local directory for repositories
pub fn sync_repo(url: &str, base_dir: &str) {
    // Step 1: Normalize the URL to ensure it has a protocol (https://)
    let url = normalize_url(url);

    // Step 2: Determine the repository name from the URL
    // Example: "https://github.com/user/repo.git" -> "repo"
    let name = url.split("/").last().unwrap().replace(".git", "");

    // Step 3: Construct the full local path for this repository
    // Example: base_dir="/home/user/repos", name="repo" -> "/home/user/repos/repo"
    let path = path::Path::new(base_dir).join(name);

    // Step 4: Check if the repository already exists locally
    if path.exists() {
        if let Err(e) = git_pull(path.to_str().unwrap()) {
            eprintln!("Error pulling {}: {}", url, e)
        }
        if let Err(e) = sync_repo_branches(path.to_str().unwrap()) {
            eprintln!("Error syncing branches in {}: {}", url, e);
        }
    } else {
        if let Err(e) = git_clone(&url, path.to_str().unwrap()) {
            eprintln!("Error cloning {}: {}", url, e)
        }
    }
}

/// Normalize a repository URL to always use HTTPS.
/// - `url`: URL
fn normalize_url(url: &str) -> String {
    // Remove leading "http://" if present
    let url = url.strip_prefix("http://").unwrap_or(url);

    // If it already starts with "https://", leave it
    if url.starts_with("https://") {
        url.to_string()
    } else {
        // Otherwise, prepend "https://"
        format!("https://{}", url)
    }
}

/// Synchronize all local branches in the repository at `path` with their upstreams.
/// - Current branch: fast-forward merge if working tree is clean
/// - Other branches: update directly from upstream without checkout
pub fn sync_repo_branches(path: &str) -> io::Result<()> {
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
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("git fetch --all failed in {}", path),
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
                let _ = process::Command::new("git")
                    .arg("-C")
                    .arg(path)
                    .arg("merge")
                    .arg("--ff-only")
                    .arg(upstream)
                    .status();
            } else {
                // Working tree dirty: skip merge to avoid conflicts
                println!("Skipping merge on {} (dirty branch)", local);
            }
        } else {
            // Non-current branch: update directly from upstream without checkout
            // Equivalent to: `git fetch . <upstream>:<local>`
            let _ = process::Command::new("git")
                .arg("-C")
                .arg(path)
                .arg("fetch")
                .arg(".")
                .arg(format!("{}:{}", upstream, local))
                .status();
        }
    }

    Ok(())
}
