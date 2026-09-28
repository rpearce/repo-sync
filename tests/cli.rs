//! Integration tests for `repo-sync`'s process-level behavior: exit codes,
//! a missing repo list, a missing `git`, and the end-of-run summary. See
//! `tests/common/mod.rs` for the shared harness.

mod common;

use common::TestEnv;

/// A missing repo-list file must be reported as a clean error, not a
/// panic. Regression test for `fs::read_to_string(...).expect(...)`,
/// which panicked (exit code 101) instead of exiting 1 with a message.
#[test]
fn missing_repo_list_is_an_error_not_a_panic() {
    let env = TestEnv::new();
    let missing = env.root().join("nope.txt");
    let out = env.root().join("out");

    let assert = env.run("sync", &missing, &out).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains(&missing.display().to_string()),
        "stderr must mention the missing path, got: {stderr:?}"
    );
    assert!(
        !stderr.contains("panicked"),
        "stderr must not contain a panic message, got: {stderr:?}"
    );
}

/// A failed clone must exit non-zero and the failure must be reflected in
/// the summary line. Regression test for the bug where a failed clone
/// only printed to stderr and the process still exited 0.
#[test]
fn failed_clone_exits_nonzero() {
    let env = TestEnv::new();
    let missing_repo = env.root().join("does-not-exist.git");
    let url = env.file_url(&missing_repo);
    let repos = env.repos_file("repos.txt", &[&url]);
    let out = env.root().join("out");

    let assert = env.run("clone", &repos, &out).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("Cloned 1 repositories: 0 ok, 1 failed"),
        "stderr must contain the summary line, got: {stderr:?}"
    );
}

/// In verbose mode, a fully successful run prints the summary line to
/// stdout.
#[test]
fn sync_reports_summary_when_verbose() {
    let env = TestEnv::new();
    let fx = env.cloned("dotfiles");

    let assert = env
        .repo_sync()
        .arg("sync")
        .arg("-f")
        .arg(&fx.repos)
        .arg("-o")
        .arg(&fx.out)
        .arg("-v")
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout
            .trim_end()
            .ends_with("Synced 1 repositories: 1 ok, 0 failed"),
        "stdout must end with the summary line, got: {stdout:?}"
    );
}

/// When `git` can't be found on `PATH`, the process must report a clear
/// error and exit non-zero, instead of the per-repo `No such file or
/// directory (os error 2)` every git invocation used to produce (with
/// exit code 0).
#[test]
fn git_missing_from_path_is_reported() {
    let env = TestEnv::new();
    let empty_path_dir = tempfile::tempdir().expect("create empty PATH dir");
    let repos = env.repos_file("repos.txt", &["file:///nonexistent.git"]);
    let out = env.root().join("out");

    let assert = env
        .repo_sync()
        .arg("sync")
        .arg("-f")
        .arg(&repos)
        .arg("-o")
        .arg(&out)
        .env("PATH", empty_path_dir.path())
        .assert()
        .failure()
        .code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("git not found on PATH"),
        "stderr must mention git not being found, got: {stderr:?}"
    );
}

/// A fully successful run without `-v` must produce no stdout output at
/// all, so `repo-sync` stays cron-friendly by default.
#[test]
fn successful_sync_is_silent_by_default() {
    let env = TestEnv::new();
    let fx = env.cloned("dotfiles");

    let assert = env.run("sync", &fx.repos, &fx.out).success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.is_empty(), "stdout must be empty, got: {stdout:?}");
}
