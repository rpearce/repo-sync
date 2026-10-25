//! Tests for the integration-test harness itself (`tests/common/mod.rs`),
//! not for `repo-sync`'s behavior.
//!
//! These guard against the harness silently reusing git state inherited
//! from the process running the tests. For example, git exports an
//! absolute `GIT_DIR` and `GIT_INDEX_FILE` to hooks, and in a linked
//! worktree `GIT_DIR` overrides `-C` entirely. If a test run inherits
//! one of these, a "sandboxed" `repo-sync` invocation can silently
//! operate on (and mutate) a real repository instead of the throwaway
//! one the test built.

mod common;

use std::ffi::OsStr;

use common::{INHERITED_GIT_ENV_VARS, TestEnv};

#[test]
fn repo_sync_command_clears_inherited_git_state_vars() {
    let env = TestEnv::new();
    let cmd = env.repo_sync();

    for key in INHERITED_GIT_ENV_VARS {
        let explicitly_removed = cmd
            .get_envs()
            .any(|(k, v)| k == OsStr::new(key) && v.is_none());
        assert!(
            explicitly_removed,
            "{key} must be explicitly removed (env_remove), not merely left unset, \
             so it overrides any value the test process itself inherited"
        );
    }
}
