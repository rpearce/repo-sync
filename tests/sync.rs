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

/// `sync` must never run git inside a directory that isn't itself a git
/// repository: git's own repository discovery walks up from `-C <dir>`
/// looking for the nearest enclosing `.git`, so treating "the target
/// directory exists" as "the target is our clone" let `sync` silently
/// fetch and fast-forward whatever repository `out` happened to be
/// sitting inside. Regression test for that bug, reproduced two ways in
/// one run: a leftover non-repo directory left over inside `out`, and a
/// bare-host entry with no path at all (which the old name-derivation
/// resolved to the empty string, and `Path::join("")` resolves to `out`
/// itself).
#[test]
fn sync_never_operates_on_enclosing_repo() {
    let env = TestEnv::new();

    // An ordinary git repo that has nothing to do with repo-sync, whose
    // own remote is about to move. `out` will live inside its working
    // tree, standing in for "some unrelated repo the user happened to
    // run repo-sync from inside".
    let enclosing_remote = env.bare_remote("enclosing");
    let enclosing = env.root().join("enclosing");
    env.git(
        env.root(),
        &[
            "clone",
            enclosing_remote.to_str().expect("utf-8 path"),
            enclosing.to_str().expect("utf-8 path"),
        ],
    );
    let enclosing_head_before = env.git(&enclosing, &["rev-parse", "HEAD"]);

    // Advance the enclosing repo's own remote. If `sync` ever ran git
    // inside (or above) `out`, git's repository discovery would find
    // `enclosing`'s `.git` and this commit would land on its HEAD.
    env.push_commit(&enclosing_remote, "main", "enclosing upstream commit");

    let out = enclosing.join("out");
    let leftover = out.join("leftover");
    fs::create_dir_all(&leftover).expect("create leftover non-repo dir inside out");

    let repos = env.repos_file(
        "repos.txt",
        &[
            // Resolves to the pre-existing plain directory above, which
            // is not a git repo: `sync` must fail closed instead of
            // running git there.
            "example.test/leftover",
            // A bare host with no path segment at all: `repo_name` must
            // reject it (`None`) rather than resolve it to the empty
            // name, which `Path::join` would turn into `out` itself.
            "example.test/",
        ],
    );

    let assert = env.run("sync", &repos, &out).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("exists but is not a git repository"),
        "stderr must report the non-repo directory, got: {stderr:?}"
    );

    let enclosing_head_after = env.git(&enclosing, &["rev-parse", "HEAD"]);
    assert_eq!(
        enclosing_head_after, enclosing_head_before,
        "sync must never move the enclosing repo's HEAD"
    );
}

/// Directory-name collisions must be caught through `sync` the same way
/// they are through `clone` — both share `partition_collisions` (see
/// `tests/cli.rs`'s `duplicate_repo_names_are_reported_not_raced`), and
/// this pins that wiring so a future refactor can't silently reintroduce
/// the race for `sync` alone.
#[test]
fn sync_reports_duplicate_repo_names_instead_of_racing() {
    let env = TestEnv::new();
    let remote_a = env.bare_remote("a/dotfiles");
    let remote_b = env.bare_remote("b/dotfiles");
    let url_a = env.file_url(&remote_a);
    let url_b = env.file_url(&remote_b);
    let repos = env.repos_file("repos.txt", &[&url_a, &url_b]);
    let out = env.root().join("out");

    let assert = env.run("sync", &repos, &out).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains(&format!(
            "'dotfiles' is used by multiple entries: {url_a}, {url_b}"
        )),
        "stderr must name both colliding entries, got: {stderr:?}"
    );
    assert!(
        !out.join("dotfiles").exists(),
        "colliding entries must not create out/dotfiles"
    );
}

/// A non-current branch that has diverged from its upstream (both sides
/// have commits the other lacks) can't be fast-forwarded by `fetch .`.
/// That must be reported as a single attributed `warning:` line naming
/// the branch, not silently ignored (the old behavior) and not one of
/// git's own raw, unattributed `hint:`/`fatal:` lines. The branch itself
/// must be left exactly as it was: a failed fast-forward is a warning,
/// never a reason to force anything.
#[test]
fn sync_warns_on_diverged_non_current_branch() {
    let env = TestEnv::new();

    // Remote `main` (from `bare_remote`) plus a `feature` branch one
    // commit ahead of it.
    let remote = env.bare_remote("dotfiles");
    env.push_commit(&remote, "feature", "feature commit 1");

    let fx = env.clone_remote(&remote);

    // Local `feature`, tracking `origin/feature`, gets its own commit
    // that the remote never sees.
    env.git(&fx.clone, &["branch", "feature", "origin/feature"]);
    env.git(&fx.clone, &["checkout", "feature"]);
    fs::write(fx.clone.join("local-feature.txt"), "local feature change\n")
        .expect("write local feature file");
    env.git(&fx.clone, &["add", "."]);
    env.git(&fx.clone, &["commit", "-m", "local feature commit"]);
    let local_feature_sha = env.git(&fx.clone, &["rev-parse", "feature"]);
    env.git(&fx.clone, &["checkout", "main"]);

    // The remote's `feature` advances from the same base with a
    // *different* commit, so the two histories genuinely diverge rather
    // than one simply being behind the other.
    env.push_commit(&remote, "feature", "feature commit 2");

    let assert = env.run("sync", &fx.repos, &fx.out).success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("warning: dotfiles: could not fast-forward feature"),
        "stderr must contain a single attributed warning naming the repo \
         and the diverged branch, got: {stderr:?}"
    );
    assert!(
        stderr.contains("non-fast-forward"),
        "the warning must end with git's actually-useful last stderr \
         line ('! [rejected] ... (non-fast-forward)'), not the \
         uninformative 'From .' header line, got: {stderr:?}"
    );
    assert!(
        !stderr.lines().any(|l| l.trim_start().starts_with("hint:")),
        "stderr must not contain git's own raw, unattributed hint: lines, \
         got: {stderr:?}"
    );

    // The local branch must be left exactly as it was: no rebase, no
    // forced update, nothing.
    let local_feature_sha_after = env.git(&fx.clone, &["rev-parse", "feature"]);
    assert_eq!(
        local_feature_sha_after, local_feature_sha,
        "a diverged non-current branch must be left untouched"
    );
}

/// The *current* branch can diverge from its upstream too (a local
/// commit plus an advanced remote). `merge --ff-only` then refuses, and
/// that must also surface as one attributed `warning:` line rather than
/// git's own multi-line, unattributed `hint:` advice text plus an
/// unattributed `fatal:` line.
#[test]
fn sync_warns_on_diverged_current_branch() {
    let env = TestEnv::new();
    let fx = env.cloned("dotfiles");

    // Commit locally on `main`, touching a file the remote's commit below
    // never touches, so the two histories diverge without conflicting.
    fs::write(fx.clone.join("local.txt"), "local change\n").expect("write local file");
    env.git(&fx.clone, &["add", "."]);
    env.git(&fx.clone, &["commit", "-m", "local commit"]);
    let local_head_before = env.git(&fx.clone, &["rev-parse", "HEAD"]);

    // Advance the remote's `main` with an unrelated commit.
    env.push_commit(&fx.remote, "main", "remote commit");

    let assert = env.run("sync", &fx.repos, &fx.out).success();

    // The local commit must survive untouched: a diverged current branch
    // is a warning, never a reason to merge, rebase or otherwise move HEAD.
    let local_head_after = env.git(&fx.clone, &["rev-parse", "HEAD"]);
    assert_eq!(
        local_head_after, local_head_before,
        "a diverged current branch must be left untouched"
    );

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("warning: dotfiles: could not fast-forward main"),
        "stderr must contain a single attributed warning naming the repo \
         and the diverged branch, got: {stderr:?}"
    );
    assert!(
        stderr.contains("Not possible to fast-forward"),
        "the warning must end with git's actual fatal: text (the last \
         non-empty stderr line), got: {stderr:?}"
    );
    assert!(
        !stderr.lines().any(|l| l.trim_start().starts_with("hint:")),
        "stderr must not contain git's own raw, unattributed hint: lines, \
         got: {stderr:?}"
    );
    assert!(
        !stderr.lines().any(|l| l.trim_start().starts_with("fatal:")),
        "stderr must not contain a bare, unattributed fatal: line (git's \
         own fatal: text may still appear inside the attributed warning \
         line itself), got: {stderr:?}"
    );
}

/// In verbose mode, a successful branch-level update must still show
/// git's own output, attributed to the repo. Regression test: making
/// failures attributable (see `sync_warns_on_diverged_non_current_branch`)
/// meant capturing branch-level commands' stdout/stderr instead of
/// inheriting them, which silently dropped the `From . ... -> feature` /
/// `Updating a..b` / `Fast-forward` progress text a successful update
/// used to print directly to the terminal in verbose mode.
#[test]
fn sync_verbose_shows_attributed_output_for_branch_update() {
    let env = TestEnv::new();

    let remote = env.bare_remote("dotfiles");
    env.push_commit(&remote, "feature", "feature commit 1");
    let fx = env.clone_remote(&remote);
    env.git(&fx.clone, &["branch", "feature", "origin/feature"]);
    env.push_commit(&remote, "feature", "feature commit 2");

    let assert = env.run_with("sync", &fx.repos, &fx.out, &["-v"]).success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    let has_attributed_line = stdout
        .lines()
        .chain(stderr.lines())
        .any(|line| line.starts_with("dotfiles: ") && line.contains("feature"));
    assert!(
        has_attributed_line,
        "verbose mode must show git's own branch-update output, \
         attributed to the repo, got stdout: {stdout:?}, stderr: {stderr:?}"
    );
}

/// A local branch whose upstream has been deleted on the remote (a
/// "gone" upstream, per `git branch -vv`) must be skipped silently
/// instead of producing a fast-forward warning on every single run.
/// Regression test: `fetch . origin/feature:feature` for such a branch
/// exits non-zero ("couldn't find remote ref"), and the old
/// `%(upstream:short)`-only format couldn't distinguish that from an
/// ordinary diverged branch, so it warned every time `sync` ran.
#[test]
fn sync_skips_branch_whose_upstream_is_gone() {
    let env = TestEnv::new();

    let remote = env.bare_remote("dotfiles");
    env.push_commit(&remote, "feature", "feature commit 1");

    let fx = env.clone_remote(&remote);
    env.git(&fx.clone, &["branch", "feature", "origin/feature"]);

    // Delete `feature` on the remote directly, so the local branch's
    // upstream becomes "[gone]" once `sync`'s own `fetch --prune` prunes
    // the now-stale `origin/feature` remote-tracking ref.
    env.git(&remote, &["branch", "-D", "feature"]);

    // `main` must still fast-forward normally in the same run.
    let remote_main_head = env.push_commit(&remote, "main", "remote commit");

    let assert = env.run("sync", &fx.repos, &fx.out).success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        !stderr.contains("feature"),
        "a [gone] upstream must produce no warning or error in \
         non-verbose mode, got: {stderr:?}"
    );

    let local_main_head = env.git(&fx.clone, &["rev-parse", "main"]);
    assert_eq!(
        local_main_head, remote_main_head,
        "main must still fast-forward even though feature's upstream is gone"
    );
}

/// A local tag literally named `origin/main` must never shadow the
/// remote-tracking branch `origin/main` when `sync` fast-forwards `main`.
/// Regression test: git's ref-disambiguation rules check `refs/tags/`
/// before `refs/remotes/`, so the old short-name refspec/merge target
/// (`origin/main`) resolved to the tag instead, silently turning the
/// fast-forward into a no-op.
#[test]
fn sync_fast_forwards_despite_ambiguous_short_ref() {
    let env = TestEnv::new();
    let fx = env.cloned("dotfiles");

    // A tag pointing at the current (old) commit, named exactly like the
    // short form of `main`'s upstream.
    env.git(&fx.clone, &["tag", "origin/main"]);

    let remote_head = env.push_commit(&fx.remote, "main", "remote commit");

    env.run("sync", &fx.repos, &fx.out).success();

    let local_head = env.git(&fx.clone, &["rev-parse", "main"]);
    assert_eq!(
        local_head, remote_head,
        "main must fast-forward to the remote's new commit even though a \
         local tag named 'origin/main' shadows the short upstream name"
    );
}

/// A clone of an empty remote has an "unborn" `HEAD`: `git symbolic-ref -q
/// HEAD` resolves it to `refs/heads/main`, but that ref doesn't exist yet
/// because nothing has ever been committed, so `git rev-parse
/// --abbrev-ref HEAD` fails there. Regression test: `sync_repo_branches`
/// used to determine the current branch via `rev-parse`, turning that
/// failure into a hard `Sync` error for every repo-list entry pointing at
/// an empty remote.
#[test]
fn sync_succeeds_on_unborn_head_after_cloning_empty_remote() {
    let env = TestEnv::new();
    let remote = env.empty_bare_remote("empty");
    let fx = env.clone_remote(&remote);

    let assert = env.run("sync", &fx.repos, &fx.out).success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        !stderr.to_lowercase().contains("error"),
        "sync of a clone with an unborn HEAD must not report an error, \
         got: {stderr:?}"
    );
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
