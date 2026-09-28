use std::path::Path;
use std::process::{self, Command, Stdio};
use std::{fmt, io};

/// Repo-locating environment variables git sets and exports to hooks
/// (e.g. an absolute `GIT_DIR`, which *overrides* `-C` entirely rather
/// than being merely redundant with it). If one of these leaks in from
/// the process running `repo-sync` — for example, `repo-sync` itself
/// invoked from a git hook, or from inside another repository's
/// working tree — every command built here would otherwise silently
/// target that repository instead of the one `-C dir` names. Removed
/// from every command this module builds.
const INHERITED_GIT_ENV_VARS: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_PREFIX",
];

/// Subcommands `git_run` appends `--quiet` for (verified: git accepts it
/// anywhere among a subcommand's own arguments, e.g. `git merge --ff-only
/// <upstream> --quiet`). Kept to exactly what `git_run` is actually
/// called with (today, just `fetch --all`): `merge` and `fetch .` are
/// branch-level calls that go through `git_output`/`git_output_combined`
/// instead (captured, not inherited, so `--quiet` wouldn't do anything),
/// and `run_clone` has no `-C` directory to route through `git_run` at
/// all, so it makes its own unconditional `--quiet` decision.
const QUIET_SUBCOMMANDS: &[&str] = &["fetch"];

/// Apply the isolation every git command this module builds needs:
/// never prompt on a terminal (`GIT_TERMINAL_PROMPT=0`, and a null
/// stdin so there's nothing to prompt on even if something ignored
/// that), and ignore repo-locating environment variables inherited
/// from the calling process (see `INHERITED_GIT_ENV_VARS`).
fn isolate(cmd: &mut Command) {
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.stdin(Stdio::null());
    for var in INHERITED_GIT_ENV_VARS {
        cmd.env_remove(var);
    }
}

/// Build an isolated `git -C dir` command (see `isolate`). Every git
/// invocation that operates on an existing repository should start
/// here. `git clone` has no existing directory to `-C` into (it
/// creates `path` itself — `-C` would fail outright if the output
/// directory doesn't exist yet), so it uses `run_clone` instead.
/// - `dir`: repository directory to run git in (`-C dir`)
pub fn git(dir: &Path) -> Command {
    let mut cmd = process::Command::new("git");
    cmd.arg("-C").arg(dir);
    isolate(&mut cmd);
    cmd
}

/// Run `git <args>` in `dir`, capturing stdout, and check the exit
/// status. On failure, returns an `Err` whose message is git's
/// captured stderr (trimmed), so a caller can both detect the failure
/// and report git's own explanation of it — a failed command is never
/// mistaken for one that merely produced empty output (e.g. a failed
/// `status --porcelain` must never be read as "clean").
/// - `dir`: repository directory to run git in (`-C dir`)
/// - `args`: arguments passed to git after `-C dir`
pub fn git_output(dir: &Path, args: &[&str]) -> io::Result<String> {
    let output = git(dir).args(args).output()?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// Like `git_output`, but on success the returned string is stdout and
/// stderr concatenated (stdout first), instead of stdout alone: some
/// subcommands write their progress to stderr even on success (`git
/// fetch`'s `From . ... -> feature` line), while others write to stdout
/// (`git merge`'s `Updating a..b` / `Fast-forward`), so a caller that
/// just wants to show the user "whatever git printed" — e.g. attributing
/// a successful branch-level update to its repo in verbose mode, see
/// `sync::print_branch_update_output` — needs both. On failure, behaves
/// exactly like `git_output`: returns `Err` with git's captured stderr.
/// - `dir`: repository directory to run git in (`-C dir`)
/// - `args`: arguments passed to git after `-C dir`
pub fn git_output_combined(dir: &Path, args: &[&str]) -> io::Result<String> {
    let output = git(dir).args(args).output()?;

    if output.status.success() {
        let mut combined = String::from_utf8_lossy(&output.stdout).into_owned();
        combined.push_str(&String::from_utf8_lossy(&output.stderr));
        Ok(combined)
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ))
    }
}

/// Run `git <args>` in `dir` with stdout/stderr inherited (so the user
/// sees git's own output directly), and check the exit status.
/// Appends `--quiet` when `verbose` is false and the subcommand
/// (`args[0]`) is one that accepts it (see `QUIET_SUBCOMMANDS`).
/// - `dir`: repository directory to run git in (`-C dir`)
/// - `args`: arguments passed to git after `-C dir`, starting with the subcommand
/// - `verbose`: when false, and the subcommand accepts it, appends `--quiet`
pub fn git_run(dir: &Path, args: &[&str], verbose: bool) -> io::Result<()> {
    let mut cmd = git(dir);
    cmd.args(args);
    if !verbose
        && args
            .first()
            .is_some_and(|sub| QUIET_SUBCOMMANDS.contains(sub))
    {
        cmd.arg("--quiet");
    }

    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "git {} failed in {}",
            args.join(" "),
            dir.display()
        )))
    }
}

/// Run `git clone <url> <path>`, with the same isolation as `git`
/// (see `isolate`) but without `-C`: unlike every other command in
/// this module, clone has no existing directory to change into first
/// (it creates `path`, including any missing parent directories,
/// itself), so it can't share `git_run`'s `-C`-based builder and makes
/// its own unconditional `--quiet` decision instead of consulting
/// `QUIET_SUBCOMMANDS`.
/// - `url`: repository URL
/// - `path`: local repository target directory
/// - `verbose`: when false, appends `--quiet`
pub fn run_clone(url: &str, path: &Path, verbose: bool) -> io::Result<()> {
    let mut cmd = process::Command::new("git");
    isolate(&mut cmd);
    cmd.arg("clone").arg(url).arg(path);
    if !verbose {
        cmd.arg("--quiet");
    }

    let status = cmd.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("git clone failed for {:?}", url)))
    }
}

/// Build an `io::Error` for a failed git command that includes the
/// command and git's stderr, so a failure is never mistaken for
/// success (e.g. a failed `status --porcelain` must never be read as
/// "clean").
/// - `command`: human-readable description of the command that failed
/// - `path`: repository directory the command ran in
/// - `stderr`: the command's error, whose message is git's captured
///   stderr (e.g. from `git_output`)
pub fn command_failed(command: &str, path: &Path, stderr: impl fmt::Display) -> io::Error {
    io::Error::other(format!("{command} failed in {}: {stderr}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn git_sets_dash_c_dir_first() {
        let dir = Path::new("/some/repo");
        let cmd = git(dir);

        let args: Vec<&OsStr> = cmd.get_args().collect();
        assert_eq!(
            &args[..2],
            &[OsStr::new("-C"), OsStr::new("/some/repo")],
            "git(dir) must start with -C <dir>, got {args:?}"
        );
    }

    #[test]
    fn git_disables_terminal_prompts() {
        let dir = Path::new("/some/repo");
        let cmd = git(dir);

        let prompt_disabled = cmd
            .get_envs()
            .any(|(k, v)| k == OsStr::new("GIT_TERMINAL_PROMPT") && v == Some(OsStr::new("0")));
        assert!(
            prompt_disabled,
            "git(dir) must set GIT_TERMINAL_PROMPT=0 so git never hangs on a prompt"
        );
    }

    #[test]
    fn git_removes_every_inherited_repo_locating_var() {
        let dir = Path::new("/some/repo");
        let cmd = git(dir);

        for var in INHERITED_GIT_ENV_VARS {
            let explicitly_removed = cmd
                .get_envs()
                .any(|(k, v)| k == OsStr::new(var) && v.is_none());
            assert!(
                explicitly_removed,
                "{var} must be explicitly removed (env_remove), got envs: {:?}",
                cmd.get_envs().collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn git_output_error_contains_git_stderr_on_failure() {
        // An empty directory with no `.git` anywhere above it (a fresh
        // tempdir) makes `rev-parse` fail predictably and portably,
        // without needing any repository setup.
        let dir = tempdir().expect("create tempdir");

        let result = git_output(dir.path(), &["rev-parse", "HEAD"]);

        let err = result.expect_err("rev-parse in a non-repo dir must fail");
        assert!(
            err.to_string().contains("not a git repository"),
            "error message must contain git's stderr, got: {err}"
        );
    }

    #[test]
    fn git_output_combined_returns_stdout_on_success() {
        let dir = tempdir().expect("create tempdir");
        let init_status = process::Command::new("git")
            .arg("init")
            .arg("-q")
            .arg(dir.path())
            .status()
            .expect("spawn git init");
        assert!(init_status.success(), "git init must succeed");

        let result = git_output_combined(dir.path(), &["rev-parse", "--show-toplevel"]);

        let output = result.expect("rev-parse --show-toplevel must succeed in a fresh repo");
        let leaf_name = dir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .expect("tempdir has a UTF-8 name");
        assert!(
            output.contains(leaf_name),
            "combined output must include the command's stdout, got: {output:?}"
        );
    }

    #[test]
    fn git_output_combined_error_contains_git_stderr_on_failure() {
        let dir = tempdir().expect("create tempdir");

        let result = git_output_combined(dir.path(), &["rev-parse", "HEAD"]);

        let err = result.expect_err("rev-parse in a non-repo dir must fail");
        assert!(
            err.to_string().contains("not a git repository"),
            "error message must contain git's stderr, got: {err}"
        );
    }

    #[test]
    fn git_run_error_message_uses_plain_args_not_debug_formatting() {
        // `status` isn't a quiet-eligible subcommand, so `verbose: true`
        // here just keeps the test focused on the error message shape,
        // not the `--quiet` logic.
        let dir = tempdir().expect("create tempdir");

        let result = git_run(dir.path(), &["status", "--porcelain"], true);

        let err = result.expect_err("status in a non-repo dir must fail");
        let message = err.to_string();
        assert!(
            message.contains("git status --porcelain failed"),
            "error message must join args with spaces (not Debug-format \
             the slice), got: {message}"
        );
        assert!(
            !message.contains('"'),
            "error message must not contain Debug-formatting quotes, got: {message}"
        );
    }
}
