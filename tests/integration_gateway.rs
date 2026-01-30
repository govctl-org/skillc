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

/// Test outline --level filters headings by level per [[RFC-0002:C-OUTLINE]]
#[test]
fn test_outline_level_filter() {
    let workspace = init_workspace();
    // --level 2 should only show # and ## headings, not ### (Prerequisites)
    let output = run_skc(
        workspace.dir.path(),
        &["outline", &workspace.skill_name, "--level", "2"],
    );
    assert_snapshot!("outline_level_2", output);
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

/// Test show --max-lines truncates output per [[RFC-0002:C-SHOW]]
#[test]
fn test_show_max_lines() {
    let workspace = init_workspace();
    // Getting Started section has multiple lines, limit to 3
    let output = run_skc(
        workspace.dir.path(),
        &[
            "show",
            &workspace.skill_name,
            "--section",
            "Getting Started",
            "--max-lines",
            "3",
        ],
    );
    assert_snapshot!("show_max_lines", output);
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

/// Test open --max-lines truncates output per [[RFC-0002:C-OPEN]]
#[test]
fn test_open_max_lines() {
    let workspace = init_workspace();
    // Limit output to 3 lines
    let output = run_skc(
        workspace.dir.path(),
        &[
            "open",
            &workspace.skill_name,
            "docs/advanced.md",
            "--max-lines",
            "3",
        ],
    );
    assert_snapshot!("open_max_lines", output);
}
