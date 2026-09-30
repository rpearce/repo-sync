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
        stderr.contains(&format!("Error cloning {url}")),
        "stderr must contain the per-repo failure line, got: {stderr:?}"
    );
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

    let assert = env.run_with("sync", &fx.repos, &fx.out, &["-v"]).success();

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

/// Two repo-list entries whose final path segment matches (e.g.
/// `.../a/dotfiles` and `.../b/dotfiles`) both resolve to the same
/// `<out>/dotfiles` directory. Before this fix they raced in the parallel
/// clone phase and the process exited 0 regardless of which one won (see
/// expert evidence B6). Now the collision is detected up front and
/// reported as a failure for both entries, `out/dotfiles` is never
/// created, and a third, non-colliding entry in the same file still
/// clones.
#[test]
fn duplicate_repo_names_are_reported_not_raced() {
    let env = TestEnv::new();
    let remote_a = env.bare_remote("a/dotfiles");
    let remote_b = env.bare_remote("b/dotfiles");
    let remote_other = env.bare_remote("other");
    let url_a = env.file_url(&remote_a);
    let url_b = env.file_url(&remote_b);
    let url_other = env.file_url(&remote_other);
    let repos = env.repos_file("repos.txt", &[&url_a, &url_b, &url_other]);
    let out = env.root().join("out");

    let assert = env.run("clone", &repos, &out).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains(&format!(
            "'dotfiles' is used by multiple entries: {url_a}, {url_b}"
        )),
        "stderr must name both colliding entries, got: {stderr:?}"
    );
    assert_eq!(
        stderr.matches("is used by multiple entries").count(),
        1,
        "the collision message must be printed exactly once, got: {stderr:?}"
    );
    assert!(
        stderr.contains("Cloned 3 repositories: 1 ok, 2 failed"),
        "stderr must contain the summary line, got: {stderr:?}"
    );

    assert!(
        !out.join("dotfiles").exists(),
        "colliding entries must not create out/dotfiles"
    );
    assert!(
        out.join("other").exists(),
        "the non-colliding entry must still clone"
    );
}

/// A directory-name collision's message must show normalized URLs for
/// every colliding entry — matching the convention `Clone`/`Sync`/
/// `NoDirectoryName` already follow — even when the raw repo-list lines
/// themselves differ (a bare `host/owner/repo` line and an `http://`
/// line that upgrades to the same `https://` URL another entry already
/// uses in normalized form).
#[test]
fn duplicate_repo_names_show_normalized_urls_in_the_message() {
    let env = TestEnv::new();
    let repos = env.repos_file(
        "repos.txt",
        &[
            "example.invalid/a/dotfiles",
            "http://example.invalid/b/dotfiles",
        ],
    );
    let out = env.root().join("out");

    let assert = env.run("clone", &repos, &out).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains(
            "'dotfiles' is used by multiple entries: \
             https://example.invalid/a/dotfiles, https://example.invalid/b/dotfiles"
        ),
        "stderr must show normalized URLs for both colliding entries, got: {stderr:?}"
    );
}

/// An exact duplicate entry (the same normalized URL listed twice) is
/// not a directory-name collision between two different repositories —
/// it's the same repository listed twice. The repeat is dropped with a
/// warning (which never changes the exit code), and the first
/// occurrence still clones normally.
#[test]
fn exact_duplicate_entry_is_ignored_with_a_warning() {
    let env = TestEnv::new();
    let remote = env.bare_remote("dotfiles");
    let url = env.file_url(&remote);
    let repos = env.repos_file("repos.txt", &[&url, &url]);
    let out = env.root().join("out");

    let assert = env.run_with("clone", &repos, &out, &["-v"]).success();

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert_eq!(
        stderr.matches("warning: duplicate entry").count(),
        1,
        "the duplicate warning must be printed exactly once, got: {stderr:?}"
    );
    assert!(
        stderr.contains(&format!("warning: duplicate entry '{url}' ignored")),
        "stderr must name the ignored duplicate, got: {stderr:?}"
    );

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(
        stdout.contains("Cloning 1 repositories"),
        "the verbose header must count only the unique entry, not the raw \
         parsed line count that still includes the dropped duplicate, got: {stdout:?}"
    );
    assert!(
        stdout.contains("Cloned 1 repositories: 1 ok, 0 failed"),
        "the summary must count only the unique entry, got: {stdout:?}"
    );

    assert!(
        out.join("dotfiles").exists(),
        "the deduplicated entry must still clone"
    );
}

/// Two entries that normalize to *different* URLs but still derive the
/// same directory name (one with a trailing `.git`, one without) remain
/// a genuine collision: exact-duplicate detection must not accidentally
/// swallow this case too.
#[test]
fn different_urls_sharing_a_name_still_collide() {
    let env = TestEnv::new();
    let remote = env.bare_remote("r");
    let url = env.file_url(&remote);
    let url_git_suffix = format!("{url}.git");
    let repos = env.repos_file("repos.txt", &[&url, &url_git_suffix]);
    let out = env.root().join("out");

    let assert = env.run("clone", &repos, &out).failure().code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains(&format!(
            "'r' is used by multiple entries: {url}, {url_git_suffix}"
        )),
        "stderr must report the collision between the two different URLs, got: {stderr:?}"
    );
    assert!(
        !out.join("r").exists(),
        "colliding entries must not create out/r"
    );
}

/// `--version` must report the crate's actual Cargo package version, not
/// a hardcoded string that silently drifts from `Cargo.toml` on release
/// (see expert evidence B3: the shipped 0.1.2 binary printed `0.1.0`).
#[test]
fn version_matches_cargo_package() {
    let env = TestEnv::new();

    let assert = env.repo_sync().arg("--version").assert().success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert_eq!(
        stdout,
        format!("repo-sync {}\n", env!("CARGO_PKG_VERSION")),
        "--version must print the Cargo package version, got: {stdout:?}"
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

/// `-v`/`--verbose` must be a global flag: it must parse whether it comes
/// before or after the subcommand. Regression test for the clap builder
/// CLI, where `verbose` was defined per-subcommand, so `-v` placed before
/// the subcommand was rejected by clap itself (exit 2, "unexpected
/// argument") instead of reaching `repo-sync`'s own error handling.
#[test]
fn verbose_is_global() {
    let env = TestEnv::new();
    let missing = env.root().join("nope.txt");
    let out = env.root().join("out");

    let assert = env
        .repo_sync()
        .args(["-v", "sync", "-f"])
        .arg(&missing)
        .arg("-o")
        .arg(&out)
        .assert()
        .failure()
        .code(1);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("cannot read repo list"),
        "a global '-v' before the subcommand must still reach repo-sync's \
         own error handling, not clap's usage error, got: {stderr:?}"
    );
    assert!(
        !stderr.contains("unexpected argument"),
        "clap must accept '-v' before the subcommand now that it's global, got: {stderr:?}"
    );
}

/// `-j`/`--jobs` must be accepted as a global flag and actually bound the
/// parallel phase without breaking a normal run: `sync -j 1` (the most
/// constrained non-zero value) over two good entries must still succeed.
#[test]
fn sync_with_jobs_flag_succeeds() {
    let env = TestEnv::new();
    let remote_a = env.bare_remote("a");
    let remote_b = env.bare_remote("b");
    let url_a = env.file_url(&remote_a);
    let url_b = env.file_url(&remote_b);
    let repos = env.repos_file("repos.txt", &[&url_a, &url_b]);
    let out = env.root().join("out");

    env.run_with("sync", &repos, &out, &["-j", "1"]).success();

    assert!(
        out.join("a").exists(),
        "-j 1 must still clone the first entry"
    );
    assert!(
        out.join("b").exists(),
        "-j 1 must still clone the second entry"
    );
}

/// Running `repo-sync` with no arguments at all must print help and exit
/// with clap's usage-error code (2): a non-`Option` `#[command(subcommand)]`
/// field implies `subcommand_required` and `arg_required_else_help`.
#[test]
fn no_args_prints_help_and_exits_2() {
    let env = TestEnv::new();

    let assert = env.repo_sync().assert().failure().code(2);

    let stderr = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        stderr.contains("Usage:"),
        "stderr must contain help/usage text, got: {stderr:?}"
    );

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.is_empty(), "stdout must be empty, got: {stdout:?}");
}
