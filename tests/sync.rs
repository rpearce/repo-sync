//! Integration tests for `repo-sync sync`, driven against local bare git
//! repos reached over `file://` URLs (see `tests/common/mod.rs`).

mod common;

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
    let remote_url = format!("file://{}", remote.display());
    let repos = env.repos_file(&[&remote_url]);

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
