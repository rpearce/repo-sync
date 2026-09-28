//! Shared fixtures for repo-sync's integration tests.
//!
//! Each integration-test binary under `tests/` compiles this module on
//! its own, so a helper this particular binary doesn't call would
//! otherwise trigger a `dead_code` warning (which fails clippy's
//! `-D warnings` gate); allow it globally instead of per-helper.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::Command as RepoSyncCommand;
use tempfile::TempDir;

/// An isolated sandbox for one test: its own `HOME`, an empty global git
/// config, and a fixed author/committer identity. Every git invocation
/// made through a `TestEnv` (directly via `git`, or indirectly by running
/// the `repo-sync` binary, whose git children inherit its environment) is
/// isolated from the developer's real git config and from other tests
/// running in parallel. Never uses `std::env::set_var`.
pub struct TestEnv {
    root: TempDir,
}

impl TestEnv {
    /// Create a new sandbox backed by a fresh temp directory.
    pub fn new() -> Self {
        let root = TempDir::new().expect("create TestEnv temp dir");
        // Write an empty (not missing) file so GIT_CONFIG_GLOBAL always
        // points at something real and git never falls back to the
        // developer's `~/.gitconfig`.
        fs::write(root.path().join("gitconfig"), "").expect("create empty global gitconfig");
        Self { root }
    }

    /// The sandbox's root directory. Repos and scratch files created by
    /// a test should live under here so they're cleaned up with it.
    pub fn root(&self) -> &Path {
        self.root.path()
    }

    /// Run `git <args>` in `dir` with the isolated environment applied,
    /// assert that it exited successfully, and return trimmed stdout.
    /// Fails the test (with git's stderr) on a non-zero exit.
    /// - `dir`: directory to run git in (`-C dir`)
    /// - `args`: arguments passed to git after `-C dir`
    pub fn git(&self, dir: &Path, args: &[&str]) -> String {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(dir).args(args);
        self.isolate(&mut cmd);

        let output = cmd.output().expect("spawn git");
        assert!(
            output.status.success(),
            "git {:?} in {:?} failed:\nstdout: {}\nstderr: {}",
            args,
            dir,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        String::from_utf8_lossy(&output.stdout).trim().to_string()
    }

    /// Create a bare repository named `name` under the sandbox, seed it
    /// with an initial commit on `main` (pushed from a throwaway working
    /// clone), and return the bare repo's path. Build a `file://` URL
    /// from this path to reach it from `repo-sync` or from `git`.
    /// - `name`: directory name for the bare repo
    pub fn bare_remote(&self, name: &str) -> PathBuf {
        let bare_path = self.root().join("remotes").join(name);
        fs::create_dir_all(bare_path.parent().expect("remotes dir has a parent"))
            .expect("create remotes directory");
        self.git(
            self.root(),
            &["init", "--bare", "-b", "main", path_str(&bare_path)],
        );

        let seed = tempfile::tempdir_in(self.root()).expect("create seed working dir");
        self.git(self.root(), &["init", "-b", "main", path_str(seed.path())]);
        self.git(
            seed.path(),
            &["remote", "add", "origin", path_str(&bare_path)],
        );
        fs::write(seed.path().join("README.md"), "seed\n").expect("write seed file");
        self.git(seed.path(), &["add", "."]);
        self.git(seed.path(), &["commit", "-m", "initial commit"]);
        self.git(seed.path(), &["push", "origin", "main"]);

        bare_path
    }

    /// Advance `branch` on `remote` by one commit, via a fresh throwaway
    /// working clone (so it never disturbs a caller's own clone of
    /// `remote`). Creates `branch` (from `remote`'s default branch) if it
    /// doesn't already exist there. Returns the new commit's sha.
    /// - `remote`: path to a bare repo, e.g. one from `bare_remote`
    /// - `branch`: branch to create or advance on `remote`
    /// - `message`: commit message (and content appended to a tracked file)
    pub fn push_commit(&self, remote: &Path, branch: &str, message: &str) -> String {
        let work = tempfile::tempdir_in(self.root()).expect("create push working dir");
        self.git(
            self.root(),
            &["clone", path_str(remote), path_str(work.path())],
        );

        let remote_ref = format!("origin/{branch}");
        if self.try_git(work.path(), &["rev-parse", "--verify", &remote_ref]) {
            self.git(work.path(), &["checkout", "-B", branch, &remote_ref]);
        } else {
            self.git(work.path(), &["checkout", "-b", branch]);
        }

        let history = work.path().join("history.txt");
        let mut contents = fs::read_to_string(&history).unwrap_or_default();
        contents.push_str(message);
        contents.push('\n');
        fs::write(&history, contents).expect("write history file");

        self.git(work.path(), &["add", "."]);
        self.git(work.path(), &["commit", "-m", message]);
        self.git(work.path(), &["push", "origin", branch]);

        self.git(work.path(), &["rev-parse", "HEAD"])
    }

    /// Write a repo-list file with one entry per line and return its path.
    /// - `entries`: repo-list lines, e.g. `file://` URLs from `bare_remote`
    pub fn repos_file(&self, entries: &[&str]) -> PathBuf {
        let path = self.root().join("repos.txt");
        fs::write(&path, entries.join("\n") + "\n").expect("write repos file");
        path
    }

    /// The `repo-sync` binary under test, with the isolated environment
    /// applied so its git children never touch real git config or state.
    /// Callers add the subcommand and its arguments, e.g.
    /// `env.repo_sync().arg("sync").arg("-f").arg(&repos).arg("-o").arg(&out)`.
    pub fn repo_sync(&self) -> RepoSyncCommand {
        let mut cmd = RepoSyncCommand::cargo_bin("repo-sync").expect("find repo-sync binary");
        for (key, value) in self.env_pairs() {
            cmd.env(key, value);
        }
        cmd
    }

    /// Apply the isolated environment variables to a plain
    /// `std::process::Command` (used by `git`).
    fn isolate(&self, cmd: &mut Command) {
        for (key, value) in self.env_pairs() {
            cmd.env(key, value);
        }
    }

    /// Run `git <args>` in `dir` under the isolated environment, without
    /// failing the test, and report whether it succeeded. Used for probes
    /// where failure is an expected outcome, not a bug (e.g. checking
    /// whether a remote branch exists yet). Output is captured (not
    /// inherited), so an expected failure doesn't spam test output.
    fn try_git(&self, dir: &Path, args: &[&str]) -> bool {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(dir).args(args);
        self.isolate(&mut cmd);
        cmd.output().expect("spawn git").status.success()
    }

    /// The environment variables that isolate git (and anything that
    /// shells out to it, like the `repo-sync` binary) from the
    /// developer's real git config, identity and HOME.
    fn env_pairs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("HOME", path_str(self.root()).to_string()),
            (
                "GIT_CONFIG_GLOBAL",
                path_str(&self.root().join("gitconfig")).to_string(),
            ),
            ("GIT_CONFIG_NOSYSTEM", "1".to_string()),
            ("GIT_AUTHOR_NAME", "repo-sync-tests".to_string()),
            (
                "GIT_AUTHOR_EMAIL",
                "repo-sync-tests@example.invalid".to_string(),
            ),
            ("GIT_COMMITTER_NAME", "repo-sync-tests".to_string()),
            (
                "GIT_COMMITTER_EMAIL",
                "repo-sync-tests@example.invalid".to_string(),
            ),
            ("GIT_TERMINAL_PROMPT", "0".to_string()),
        ]
    }
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::new()
    }
}

/// `Path::to_str`, panicking with a clear message on non-UTF-8 paths
/// (test fixtures live under a temp dir and never need non-UTF-8 paths).
fn path_str(path: &Path) -> &str {
    path.to_str().expect("non-UTF-8 path in test fixture")
}
