//! Integration tests for `repo-sync sync`, driven against local bare git
//! repos reached over `file://` URLs (see `tests/common/mod.rs`).

mod common;

use std::fs;

use common::TestEnv;

/// Non-current tracking branches must be updated by `sync` even when
/// `-v` isn't passed. Regression test for the bug where `for-each-ref
/// --quiet` (an invalid flag for that subcommand) made git exit non-zero,
/// leaving `branch_pairs` empty and the whole branch-update loop a no-op.
#[test]
fn sync_fast_forwards_non_current_tracking_branch_without_verbose() {
    let env = TestEnv::new();

    // 1. Create a remote with `main` (from `bare_remote`) and `feature`.
    let remote = env.bare_remote("dotfiles");
    env.push_commit(&remote, "feature", "feature commit 1");

    let out_dir = env.root().join("out");
    let remote_url = env.file_url(&remote);
    let repos = env.repos_file("repos.txt", &[&remote_url]);

    // 2. Clone it with repo-sync.
    env.repo_sync()
        .arg("clone")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    let clone_path = out_dir.join("dotfiles");

    // 3. Make `feature` a local branch tracking `origin/feature`.
    env.git(&clone_path, &["branch", "feature", "origin/feature"]);

    // 4. Push a new commit to the remote's `feature`.
    let remote_feature_sha = env.push_commit(&remote, "feature", "feature commit 2");

    // 5. Sync without -v.
    env.repo_sync()
        .arg("sync")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    // 6. The local `feature` branch must have fast-forwarded to match
    // the remote, even though it was never checked out and `-v` was
    // never passed.
    let local_feature_sha = env.git(&clone_path, &["rev-parse", "feature"]);
    assert_eq!(local_feature_sha, remote_feature_sha);
}

/// `sync` must never invoke a plain `git pull`, because a `pull.rebase =
/// true` in the user's config makes it rewrite local commits instead of
/// fast-forwarding. Regression test for the bug where `sync_repo` ran
/// `git pull` (honoring the user's pull config) ahead of the fast-forward
/// merge in `sync_repo_branches`.
#[test]
fn sync_never_rebases_local_commits() {
    let env = TestEnv::new();

    // Write `pull.rebase = true` into the isolated global git config that
    // `TestEnv` points every git invocation at via `GIT_CONFIG_GLOBAL`.
    fs::write(env.root().join("gitconfig"), "[pull]\n\trebase = true\n")
        .expect("write pull.rebase into isolated global gitconfig");

    let remote = env.bare_remote("dotfiles");

    let out_dir = env.root().join("out");
    let remote_url = env.file_url(&remote);
    let repos = env.repos_file("repos.txt", &[&remote_url]);

    env.repo_sync()
        .arg("clone")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    let clone_path = out_dir.join("dotfiles");

    // Commit locally on `main`, touching a file the remote's commit below
    // never touches, so the two histories diverge without conflicting.
    fs::write(clone_path.join("local.txt"), "local change\n").expect("write local file");
    env.git(&clone_path, &["add", "."]);
    env.git(&clone_path, &["commit", "-m", "local commit"]);
    let local_head_before = env.git(&clone_path, &["rev-parse", "HEAD"]);

    // Advance the remote's `main` with an unrelated commit, so the local
    // and remote histories have diverged from a common ancestor.
    env.push_commit(&remote, "main", "remote commit");

    env.repo_sync()
        .arg("sync")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    // The local commit must survive untouched: no rebase, no merge commit.
    let local_head_after = env.git(&clone_path, &["rev-parse", "HEAD"]);
    assert_eq!(local_head_after, local_head_before);
}

/// `sync` must skip the fast-forward merge entirely when the current
/// branch has a tracked-file modification, even when that modification
/// doesn't textually conflict with the incoming commit. Regression test
/// for the bug where `git pull` ran ahead of the clean check and merged
/// into a dirty working tree.
#[test]
fn sync_skips_current_branch_with_tracked_modifications() {
    let env = TestEnv::new();

    let remote = env.bare_remote("dotfiles");

    let out_dir = env.root().join("out");
    let remote_url = env.file_url(&remote);
    let repos = env.repos_file("repos.txt", &[&remote_url]);

    env.repo_sync()
        .arg("clone")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    let clone_path = out_dir.join("dotfiles");
    let head_before = env.git(&clone_path, &["rev-parse", "HEAD"]);

    // Advance the remote with a commit to a different file than the one
    // we're about to dirty locally.
    env.push_commit(&remote, "main", "remote commit");

    // Modify a tracked file (from the seed commit) without committing.
    let modified_contents = "seed\nlocal edit\n";
    fs::write(clone_path.join("README.md"), modified_contents).expect("modify tracked file");

    env.repo_sync()
        .arg("sync")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    let head_after = env.git(&clone_path, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_after, head_before,
        "HEAD must not move on a dirty tree"
    );

    let contents_after =
        fs::read_to_string(clone_path.join("README.md")).expect("read tracked file");
    assert_eq!(
        contents_after, modified_contents,
        "the tracked modification must survive the skipped merge"
    );
}

/// Untracked files must never block the fast-forward merge: `merge
/// --ff-only` itself refuses to overwrite an untracked file that's in the
/// way, so the clean check only needs to consider tracked modifications.
/// Regression guard: this already passes today (via `git pull`), and must
/// keep passing once `pull` is replaced with `merge --ff-only`.
#[test]
fn sync_fast_forwards_current_branch_with_only_untracked_files() {
    let env = TestEnv::new();

    let remote = env.bare_remote("dotfiles");

    let out_dir = env.root().join("out");
    let remote_url = env.file_url(&remote);
    let repos = env.repos_file("repos.txt", &[&remote_url]);

    env.repo_sync()
        .arg("clone")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    let clone_path = out_dir.join("dotfiles");

    let remote_head = env.push_commit(&remote, "main", "remote commit");

    // Add an untracked file that has nothing to do with the incoming
    // change.
    fs::write(clone_path.join("untracked.txt"), "not tracked\n").expect("write untracked file");

    env.repo_sync()
        .arg("sync")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out_dir)
        .assert()
        .success();

    let local_head_after = env.git(&clone_path, &["rev-parse", "HEAD"]);
    assert_eq!(local_head_after, remote_head);
}
