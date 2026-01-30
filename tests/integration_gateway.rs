//! CLI integration tests for gateway commands.

mod common;

use common::{init_workspace, run_skc};
use insta::assert_snapshot;

#[test]
fn test_outline_snapshot() {
    let workspace = init_workspace();
    // Use skill name, not path (run from project directory)
    let output = run_skc(workspace.dir.path(), &["outline", &workspace.skill_name]);
    assert_snapshot!("outline", output);
}

#[test]
fn test_show_snapshot() {
    let workspace = init_workspace();
    // Use skill name, not path (run from project directory)
    let output = run_skc(
        workspace.dir.path(),
        &[
            "show",
            &workspace.skill_name,
            "--section",
            "Getting Started",
        ],
    );
    assert_snapshot!("show", output);
}

#[test]
fn test_open_snapshot() {
    let workspace = init_workspace();
    // Use skill name, not path (run from project directory)
    let output = run_skc(
        workspace.dir.path(),
        &["open", &workspace.skill_name, "docs/advanced.md"],
    );
    assert_snapshot!("open", output);
}
