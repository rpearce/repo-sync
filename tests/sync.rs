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

    // 1. Create a remote with `main` (from `bare_remote`) and `feature`,
    // before cloning, so `origin/feature` exists right after the clone.
    let remote = env.bare_remote("dotfiles");
    env.push_commit(&remote, "feature", "feature commit 1");

    // 2. Clone it with repo-sync.
    let fx = env.clone_remote(&remote);

    // 3. Make `feature` a local branch tracking `origin/feature`.
    env.git(&fx.clone, &["branch", "feature", "origin/feature"]);

    // 4. Push a new commit to the remote's `feature`.
    let remote_feature_sha = env.push_commit(&remote, "feature", "feature commit 2");

    // 5. Sync without -v.
    env.run("sync", &fx.repos, &fx.out).success();

    // 6. The local `feature` branch must have fast-forwarded to match
    // the remote, even though it was never checked out and `-v` was
    // never passed.
    let local_feature_sha = env.git(&fx.clone, &["rev-parse", "feature"]);
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
    env.set_global_config("pull.rebase", "true");

    let fx = env.cloned("dotfiles");

    // Commit locally on `main`, touching a file the remote's commit below
    // never touches, so the two histories diverge without conflicting.
    fs::write(fx.clone.join("local.txt"), "local change\n").expect("write local file");
    env.git(&fx.clone, &["add", "."]);
    env.git(&fx.clone, &["commit", "-m", "local commit"]);
    let local_head_before = env.git(&fx.clone, &["rev-parse", "HEAD"]);

    // Advance the remote's `main` with an unrelated commit, so the local
    // and remote histories have diverged from a common ancestor.
    let remote_head = env.push_commit(&fx.remote, "main", "remote commit");

    env.run("sync", &fx.repos, &fx.out).success();

    // The local commit must survive untouched: no rebase, no merge commit.
    let local_head_after = env.git(&fx.clone, &["rev-parse", "HEAD"]);
    assert_eq!(local_head_after, local_head_before);

    // `sync` must still have fetched, even though it left the current
    // branch alone: `origin/main` should reflect the remote's new commit.
    let origin_main = env.git(&fx.clone, &["rev-parse", "origin/main"]);
    assert_eq!(origin_main, remote_head);
}

/// `sync` must skip the fast-forward merge entirely when the current
/// branch has a tracked-file modification, even when that modification
/// doesn't textually conflict with the incoming commit. Regression test
/// for the bug where `git pull` ran ahead of the clean check and merged
/// into a dirty working tree.
#[test]
fn sync_skips_current_branch_with_tracked_modifications() {
    let env = TestEnv::new();
    let fx = env.cloned("dotfiles");

    let head_before = env.git(&fx.clone, &["rev-parse", "HEAD"]);

    // Advance the remote with a commit to a different file than the one
    // we're about to dirty locally.
    let remote_head = env.push_commit(&fx.remote, "main", "remote commit");

    // Modify a tracked file (from the seed commit) without committing.
    let modified_contents = "seed\nlocal edit\n";
    fs::write(fx.clone.join("README.md"), modified_contents).expect("modify tracked file");

    env.run("sync", &fx.repos, &fx.out).success();

    let head_after = env.git(&fx.clone, &["rev-parse", "HEAD"]);
    assert_eq!(
        head_after, head_before,
        "HEAD must not move on a dirty tree"
    );

    let contents_after = fs::read_to_string(fx.clone.join("README.md")).expect("read tracked file");
    assert_eq!(
        contents_after, modified_contents,
        "the tracked modification must survive the skipped merge"
    );

    // `sync` must still have fetched, even though it skipped the merge:
    // `origin/main` should reflect the remote's new commit.
    let origin_main = env.git(&fx.clone, &["rev-parse", "origin/main"]);
    assert_eq!(origin_main, remote_head);
}

/// Untracked files must never block the fast-forward merge: `merge
/// --ff-only` itself refuses to overwrite an untracked file that's in the
/// way, so the clean check only needs to consider tracked modifications.
/// Regression guard: this already passes today (via `git pull`), and must
/// keep passing once `pull` is replaced with `merge --ff-only`.
#[test]
fn sync_fast_forwards_current_branch_with_only_untracked_files() {
    let env = TestEnv::new();
    let fx = env.cloned("dotfiles");

    let remote_head = env.push_commit(&fx.remote, "main", "remote commit");

    // Add an untracked file that has nothing to do with the incoming
    // change.
    fs::write(fx.clone.join("untracked.txt"), "not tracked\n").expect("write untracked file");

    env.run("sync", &fx.repos, &fx.out).success();

    let local_head_after = env.git(&fx.clone, &["rev-parse", "HEAD"]);
    assert_eq!(local_head_after, remote_head);
}

/// `sync` must never delete a local tag that was never pushed to the
/// remote. Regression test for the bug where `git fetch --all -Pp`
/// includes `--prune-tags`, which deletes any local tag the remote
/// doesn't have — including one that only ever existed locally.
#[test]
fn sync_preserves_local_tags() {
    let env = TestEnv::new();
    let fx = env.cloned("dotfiles");

    // Create a tag locally that is never pushed to the remote.
    env.git(&fx.clone, &["tag", "local-only-tag"]);

    env.run("sync", &fx.repos, &fx.out).success();

    let tags = env.git(&fx.clone, &["tag", "--list"]);
    assert!(
        tags.lines().any(|t| t == "local-only-tag"),
        "local-only-tag must survive sync, got tags: {tags:?}"
    );
}
