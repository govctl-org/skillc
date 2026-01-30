//! CLI integration tests for stats command.

mod common;

use common::{init_workspace, run_skc, run_skc_allow_fail, run_skc_json};
use insta::assert_snapshot;

fn seed_logs() -> common::TestWorkspace {
    let workspace = init_workspace();

    // Use skill name, not path (run from project directory)
    let skill = &workspace.skill_name;
    let root = workspace.dir.path();

    let _ = run_skc(root, &["outline", skill]);
    let _ = run_skc(root, &["show", skill, "--section", "Getting Started"]);
    let _ = run_skc(root, &["open", skill, "docs/advanced.md"]);
    let _ = run_skc_allow_fail(root, &["show", skill, "--section", "Missing"]);

    workspace
}

#[test]
fn test_stats_summary_text() {
    let workspace = seed_logs();
    // Use skill name, not path
    let output = run_skc(workspace.dir.path(), &["stats", &workspace.skill_name]);
    assert_snapshot!("summary_text", output);
}

#[test]
fn test_stats_summary_json() {
    let workspace = seed_logs();
    // Use skill name, not path
    let output = run_skc_json(
        workspace.dir.path(),
        &["stats", &workspace.skill_name, "--format", "json"],
    );
    assert_snapshot!("summary_json", output);
}

#[test]
fn test_stats_queries_json() {
    let workspace = seed_logs();
    // Use skill name, not path
    let skill = &workspace.skill_name;
    let root = workspace.dir.path();

    let sections = run_skc_json(
        root,
        &["stats", skill, "--group-by", "sections", "--format", "json"],
    );
    assert_snapshot!("sections_json", sections);

    let files = run_skc_json(
        root,
        &["stats", skill, "--group-by", "files", "--format", "json"],
    );
    assert_snapshot!("files_json", files);

    let commands = run_skc_json(
        root,
        &["stats", skill, "--group-by", "commands", "--format", "json"],
    );
    assert_snapshot!("commands_json", commands);

    let projects = run_skc_json(
        root,
        &["stats", skill, "--group-by", "projects", "--format", "json"],
    );
    assert_snapshot!("projects_json", projects);

    let errors = run_skc_json(
        root,
        &["stats", skill, "--group-by", "errors", "--format", "json"],
    );
    assert_snapshot!("errors_json", errors);
}
