//! Integration tests for `repo-sync clone`, driven against local bare git
//! repos reached over `file://` URLs (see `tests/common/mod.rs`).

mod common;

use common::TestEnv;

/// A repo-list entry must never reach `git clone` as an option: an entry
/// like `--upload-pack=<cmd>;://...` would otherwise run `<cmd>`.
#[test]
fn clone_never_passes_an_entry_to_git_as_an_option() {
    let env = TestEnv::new();
    let marker = env.root().join("injected");
    let entry = format!("--upload-pack=touch '{}';://h/a:b", marker.display());
    let repos = env.repos_file("repos.txt", &[&entry]);

    let _ = env.run("clone", &repos, &env.root().join("out"));

    assert!(
        !marker.exists(),
        "a repo-list entry was passed to git as an option and ran a command"
    );
}
