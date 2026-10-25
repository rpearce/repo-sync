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

/// Git environment variables that carry repository state and might be
/// inherited from the process running the tests — for example, git
/// exports an absolute `GIT_DIR` and `GIT_INDEX_FILE` to hooks, and in a
/// linked worktree `GIT_DIR` overrides `-C` entirely. Left set, they can
/// make an isolated command silently operate on (and mutate) a real
/// repository instead of the sandbox. Removed (not just overridden) from
/// every command `isolate` touches. `pub` so `tests/harness.rs` can
/// assert against this exact list instead of keeping its own copy.
pub const INHERITED_GIT_ENV_VARS: &[&str] = &[
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

/// A repository cloned by `repo-sync clone` for a `sync` test, bundling
/// the pieces most such tests need so they don't each repeat the
/// bare-remote-plus-clone boilerplate. Built by `TestEnv::cloned` or
/// `TestEnv::clone_remote`.
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

    /// Write `key = value` into the sandbox's isolated global git config,
    /// via `git config --global` (rather than a test poking the config
    /// file directly), so it stays consistent with whatever else git
    /// writes there.
    /// - `key`: config key, e.g. `"pull.rebase"`
    /// - `value`: config value, e.g. `"true"`
    pub fn set_global_config(&self, key: &str, value: &str) {
        self.git(self.root(), &["config", "--global", key, value]);
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

    /// Create an empty bare repository named `name` under the sandbox: no
    /// seed commit, unlike `bare_remote`. Its `HEAD` still points at a
    /// branch (`main`) via a symbolic ref, but that branch doesn't exist
    /// as an actual ref yet ("unborn"), since nothing has ever been
    /// committed. A clone of this remote inherits the same unborn `HEAD`,
    /// which `git rev-parse --abbrev-ref HEAD` fails on (while
    /// `git symbolic-ref -q HEAD` succeeds).
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
    /// entry per line, and return its path. Takes a name, rather than
    /// always writing to the same file, so a single test can build more
    /// than one repo-list without one overwriting another.
    /// - `name`: file name to create under the sandbox root
    /// - `entries`: repo-list lines, e.g. `file://` URLs from `file_url`
    pub fn repos_file(&self, name: &str, entries: &[&str]) -> PathBuf {
        let path = self.root().join(name);
        fs::write(&path, entries.join("\n") + "\n").expect("write repos file");
        path
    }

    /// The `repo-sync` binary under test, with the isolated environment
    /// applied so its git children never touch real git config or state,
    /// and a default 60-second timeout so a hang fails the test instead
    /// of stalling CI. Callers add the subcommand and its arguments, e.g.
    /// `env.repo_sync().arg("sync").arg("-f").arg(&repos).arg("-o").arg(&out)`.
    pub fn repo_sync(&self) -> RepoSyncCommand {
        // Build as a plain `std::process::Command` first so it goes
        // through the same `isolate` as every `git` call, then hand it
        // to `assert_cmd` (which has no way to mutate an existing
        // `assert_cmd::Command`'s inner `std::process::Command`).
        let mut std_cmd = Command::cargo_bin("repo-sync").expect("find repo-sync binary");
        self.isolate(&mut std_cmd);

        let mut cmd = RepoSyncCommand::from_std(std_cmd);
        cmd.timeout(Duration::from_secs(60));
        cmd
    }

    /// Run `repo-sync <sub> -f <repos> -o <out>` through the isolated
    /// environment and return the resulting `Assert`, for the caller to
    /// finish (e.g. `.success()`). Shared by every test that drives
    /// `clone` or `sync` so the argument wiring lives in one place.
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

    /// Clone an existing bare `remote` (e.g. from `bare_remote`) with
    /// `repo-sync clone`, asserted to succeed, into the sandbox's shared
    /// `out` directory, and bundle the result into a `Fixture`. Kept
    /// separate from `cloned` so a test can seed remote branches (e.g.
    /// via `push_commit`) before the clone happens.
    ///
    /// The clone's directory name is derived from `remote`'s own file
    /// name (stripping a trailing `.git`), matching how `repo-sync` itself
    /// names the clone from the URL's last path segment. There is no
    /// separate `name` parameter: one that disagreed with `remote`'s
    /// actual file name would make `Fixture.clone` point at a directory
    /// `repo-sync` never created.
    /// - `remote`: path to a bare repo, e.g. one from `bare_remote`
    pub fn clone_remote(&self, remote: &Path) -> Fixture {
        let out = self.root().join("out");
        let remote_url = self.file_url(remote);
        let remote_file_name = remote
            .file_name()
            .and_then(|n| n.to_str())
            .expect("remote path has a UTF-8 file name");
        let name = remote_file_name
            .strip_suffix(".git")
            .unwrap_or(remote_file_name);
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

    /// Apply the isolated environment to a `std::process::Command`: unset
    /// any inherited git repository state (`INHERITED_GIT_ENV_VARS`),
    /// then set the sandbox's own `HOME`, git config and identity. Used
    /// by both `git`/`try_git` and, via `Command::cargo_bin`, by
    /// `repo_sync`, so there is exactly one place that does this.
    fn isolate(&self, cmd: &mut Command) {
        for key in INHERITED_GIT_ENV_VARS {
            cmd.env_remove(key);
        }
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
