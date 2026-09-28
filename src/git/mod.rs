use std::{io, process};

pub mod clone;
pub mod sync;

/// Verify that `git` is available on `PATH`, by attempting to spawn `git
/// --version`. Call once at startup: without this, a missing `git`
/// surfaces only as a cryptic "No such file or directory" from the first
/// per-repo git invocation, repeated once per repo.
pub fn preflight() -> io::Result<()> {
    process::Command::new("git")
        .arg("--version")
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status()
        .map(|_| ())
}
