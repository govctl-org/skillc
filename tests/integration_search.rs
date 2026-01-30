//! CLI integration tests for search command per RFC-0004.

mod common;

use common::{init_workspace, run_skc, run_skc_json};
use insta::assert_snapshot;

/// Build the skill to create search index.
fn build_skill(workspace: &common::TestWorkspace) {
    // Create mock agent directory to avoid writing to real ~/.claude/skills/
    let mock_agent = workspace.dir.path().join("mock-agent");
    std::fs::create_dir_all(&mock_agent).expect("create mock agent dir");

    // Use skill name since the skill is in the proper project location (.skillc/skills/)
    run_skc(
        workspace.dir.path(),
        &[
            "build",
            &workspace.skill_name,
            "--target",
            mock_agent.to_str().expect("mock agent path is UTF-8"),
        ],
    );
}

#[test]
fn test_search_text_output() {
    let workspace = init_workspace();
    build_skill(&workspace);

    let output = run_skc(
        workspace.dir.path(),
        &["search", &workspace.skill_name, "performance tips"],
    );
    assert_snapshot!("search_text", output);
}

#[test]
fn test_search_json_output() {
    let workspace = init_workspace();
    build_skill(&workspace);

    let output = run_skc_json(
        workspace.dir.path(),
        &[
            "search",
            &workspace.skill_name,
            "performance",
            "--format",
            "json",
        ],
    );
    assert_snapshot!("search_json", output);
}

#[test]
fn test_search_no_results() {
    let workspace = init_workspace();
    build_skill(&workspace);

    let output = run_skc_json(
        workspace.dir.path(),
        &[
            "search",
            &workspace.skill_name,
            "nonexistent_query_xyz123",
            "--format",
            "json",
        ],
    );
    assert_snapshot!("search_no_results", output);
}

#[test]
fn test_search_limit() {
    let workspace = init_workspace();
    build_skill(&workspace);

    let output = run_skc_json(
        workspace.dir.path(),
        &[
            "search",
            &workspace.skill_name,
            "test",
            "--format",
            "json",
            "--limit",
            "2",
        ],
    );
    assert_snapshot!("search_limit", output);
}
