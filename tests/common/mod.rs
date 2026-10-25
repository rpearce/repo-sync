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
use std::time::Duration;

use assert_cmd::Command as RepoSyncCommand;
use assert_cmd::assert::Assert;
use assert_cmd::cargo::CommandCargoExt;
use tempfile::TempDir;

/// Inherited environment variables that would point git at state outside
/// the sandbox: a repository (`GIT_DIR` and friends, which git exports to
/// hooks and which override `-C`), injected config (`GIT_CONFIG_*`), or the
/// global ignore and attributes files (`XDG_CONFIG_HOME`). `isolate`
/// removes them from every command it builds.
const INHERITED_GIT_ENV_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "XDG_CONFIG_HOME",
];

/// A repository cloned by `repo-sync clone`, plus what a test needs to sync
/// it again. Built by `TestEnv::cloned` or `TestEnv::clone_remote`.
pub struct Fixture {
    /// The bare remote the clone came from (e.g. for `push_commit`).
    pub remote: PathBuf,
    /// The repo-list file `repo-sync` was pointed at to create this clone
    /// (reusable for a later `sync` invocation via `TestEnv::run`).
    pub repos: PathBuf,
    /// The output directory `repo-sync` cloned into.
    pub out: PathBuf,
    /// The clone's local directory, under `out`.
    pub clone: PathBuf,
}

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
        let output = self.git_command(dir, args).output().expect("spawn git");
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

    /// Set `key` to `value` in the sandbox's global git config.
    /// - `key`: config key, e.g. `"pull.rebase"`
    /// - `value`: config value, e.g. `"true"`
    pub fn set_global_config(&self, key: &str, value: &str) {
        self.git(self.root(), &["config", "--global", key, value]);
    }

    /// Create a bare repository named `name` under the sandbox with one
    /// commit on `main`, and return its path. The seed commit adds
    /// `README.md`, which is deliberately not the file `push_commit` edits,
    /// so a test can dirty one while the remote changes the other.
    /// - `name`: directory name for the bare repo
    pub fn bare_remote(&self, name: &str) -> PathBuf {
        let bare_path = self.empty_bare_remote(name);

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

    /// Create an empty bare repository named `name` under the sandbox, and
    /// return its path. It has no commits, so its `main` branch (and that
    /// of any clone) is unborn.
    /// - `name`: directory name for the bare repo
    pub fn empty_bare_remote(&self, name: &str) -> PathBuf {
        let bare_path = self.root().join("remotes").join(name);
        fs::create_dir_all(bare_path.parent().expect("remotes dir has a parent"))
            .expect("create remotes directory");
        self.git(
            self.root(),
            &["init", "--bare", "-b", "main", path_str(&bare_path)],
        );

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

    /// Build a `file://` URL for `path`, usable as a repo-list entry or
    /// as a `git` remote.
    /// - `path`: an absolute local path, e.g. one returned by `bare_remote`
    pub fn file_url(&self, path: &Path) -> String {
        format!("file://{}", path.display())
    }

    /// Write a repo-list file named `name` (e.g. `"repos.txt"`) with one
    /// entry per line, and return its path.
    /// - `name`: file name to create under the sandbox root
    /// - `entries`: repo-list lines, e.g. `file://` URLs from `file_url`
    pub fn repos_file(&self, name: &str, entries: &[&str]) -> PathBuf {
        let path = self.root().join(name);
        fs::write(&path, entries.join("\n") + "\n").expect("write repos file");
        path
    }

    /// The `repo-sync` binary under test, isolated like every `git` call,
    /// with a 60-second timeout so a hang fails the test instead of stalling
    /// CI. Callers add the subcommand and its arguments.
    pub fn repo_sync(&self) -> RepoSyncCommand {
        // `isolate` takes a `std::process::Command`, so build one and wrap it
        let mut std_cmd = Command::cargo_bin("repo-sync").expect("find repo-sync binary");
        self.isolate(&mut std_cmd);

        let mut cmd = RepoSyncCommand::from_std(std_cmd);
        cmd.timeout(Duration::from_secs(60));
        cmd
    }

    /// Run `repo-sync <sub> -f <repos> -o <out>` and return the resulting
    /// `Assert`, for the caller to finish (e.g. `.success()`).
    /// - `sub`: subcommand, e.g. `"clone"` or `"sync"`
    /// - `repos`: repo-list file, e.g. from `repos_file` or a `Fixture`
    /// - `out`: output directory, e.g. from a `Fixture`
    pub fn run(&self, sub: &str, repos: &Path, out: &Path) -> Assert {
        self.run_with(sub, repos, out, &[])
    }

    /// Like `run`, but for a test that needs extra flags beyond `-f` and
    /// `-o` (e.g. `-v`).
    /// - `sub`: subcommand, e.g. `"clone"` or `"sync"`
    /// - `repos`: repo-list file, e.g. from `repos_file` or a `Fixture`
    /// - `out`: output directory, e.g. from a `Fixture`
    /// - `extra`: additional arguments, e.g. `&["-v"]`
    pub fn run_with(&self, sub: &str, repos: &Path, out: &Path, extra: &[&str]) -> Assert {
        self.repo_sync()
            .arg(sub)
            .arg("-f")
            .arg(repos)
            .arg("-o")
            .arg(out)
            .args(extra)
            .assert()
    }

    /// Clone `remote` with `repo-sync clone` (asserted to succeed) into the
    /// sandbox's `out` directory, and return the resulting `Fixture`.
    /// Separate from `cloned` so a test can change the remote (e.g. with
    /// `push_commit`) before cloning.
    /// - `remote`: path to a bare repo whose file name has no `.git` suffix,
    ///   so it's also the clone's directory name, e.g. one from `bare_remote`
    pub fn clone_remote(&self, remote: &Path) -> Fixture {
        let out = self.root().join("out");
        let remote_url = self.file_url(remote);
        let name = remote
            .file_name()
            .and_then(|n| n.to_str())
            .expect("remote path has a UTF-8 file name");
        let repos = self.repos_file(&format!("{name}.repos.txt"), &[&remote_url]);

        self.run("clone", &repos, &out).success();

        Fixture {
            remote: remote.to_path_buf(),
            repos,
            out: out.clone(),
            clone: out.join(name),
        }
    }

    /// Create a bare remote named `name` and immediately clone it (see
    /// `clone_remote`). Convenience for the common case where nothing
    /// needs to happen on the remote before the clone.
    /// - `name`: directory name for both the bare remote and its clone
    pub fn cloned(&self, name: &str) -> Fixture {
        let remote = self.bare_remote(name);
        self.clone_remote(&remote)
    }

    /// Apply the sandbox to `cmd`: remove inherited git state
    /// (`INHERITED_GIT_ENV_VARS`), then point `HOME`, git's config and the
    /// commit identity at the sandbox.
    fn isolate(&self, cmd: &mut Command) {
        for key in INHERITED_GIT_ENV_VARS {
            cmd.env_remove(key);
        }
        cmd.env("HOME", self.root())
            .env("GIT_CONFIG_GLOBAL", self.root().join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "repo-sync-tests")
            .env("GIT_AUTHOR_EMAIL", "repo-sync-tests@example.invalid")
            .env("GIT_COMMITTER_NAME", "repo-sync-tests")
            .env("GIT_COMMITTER_EMAIL", "repo-sync-tests@example.invalid")
            .env("GIT_TERMINAL_PROMPT", "0");
    }

    /// Like `git`, but for probes where failure is expected: report whether
    /// it succeeded instead of failing the test.
    fn try_git(&self, dir: &Path, args: &[&str]) -> bool {
        self.git_command(dir, args)
            .output()
            .expect("spawn git")
            .status
            .success()
    }

    /// Build `git -C <dir> <args>` with the sandbox applied.
    fn git_command(&self, dir: &Path, args: &[&str]) -> Command {
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(dir).args(args);
        self.isolate(&mut cmd);
        cmd
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
