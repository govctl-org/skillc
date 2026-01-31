//! CLI integration tests for stats command.

mod common;

use common::TestContext;
use insta::assert_snapshot;

fn seed_logs(ctx: &TestContext) {
    let skill = ctx.skill_name();

    let _ = ctx.run_skc(&["outline", skill]);
    let _ = ctx.run_skc(&["show", skill, "--section", "Getting Started"]);
    let _ = ctx.run_skc(&["open", skill, "docs/advanced.md"]);
    // Allow failure for missing section
    let _ = ctx.run_skc(&["show", skill, "--section", "Missing"]);
}

fn seed_logs_with_search(ctx: &TestContext) {
    let skill = ctx.skill_name();

    // Build the skill first (search requires a search index)
    let result = ctx.run_skc(&["build", skill, "--target", ctx.mock_agent_str()]);
    result.assert_success("build for search");

    // Generate some search queries for stats --group-by search
    let _ = ctx.run_skc(&["search", skill, "getting started"]);
    let _ = ctx.run_skc(&["search", skill, "api"]);
    let _ = ctx.run_skc(&["search", skill, "getting started"]); // duplicate query
    let _ = ctx.run_skc(&["search", skill, "performance"]);
}

#[test]
fn test_stats_summary_text() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    seed_logs(&ctx);
    let output = ctx.run_skc_text(&["stats", ctx.skill_name()]);
    assert_snapshot!("summary_text", output);
}

#[test]
fn test_stats_summary_json() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    seed_logs(&ctx);
    let output = ctx.run_skc_json(&["stats", ctx.skill_name(), "--format", "json"]);
    assert_snapshot!("summary_json", output);
}

#[test]
fn test_stats_queries_json() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    seed_logs(&ctx);
    let skill = ctx.skill_name();

    let sections =
        ctx.run_skc_json(&["stats", skill, "--group-by", "sections", "--format", "json"]);
    assert_snapshot!("sections_json", sections);

    let files = ctx.run_skc_json(&["stats", skill, "--group-by", "files", "--format", "json"]);
    assert_snapshot!("files_json", files);

    let commands =
        ctx.run_skc_json(&["stats", skill, "--group-by", "commands", "--format", "json"]);
    assert_snapshot!("commands_json", commands);

    let projects =
        ctx.run_skc_json(&["stats", skill, "--group-by", "projects", "--format", "json"]);
    assert_snapshot!("projects_json", projects);

    let errors = ctx.run_skc_json(&["stats", skill, "--group-by", "errors", "--format", "json"]);
    assert_snapshot!("errors_json", errors);
}

/// Test stats --group-by search per [[RFC-0003:C-QUERIES]]
#[test]
fn test_stats_search_json() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    seed_logs_with_search(&ctx);

    let search = ctx.run_skc_json(&[
        "stats",
        ctx.skill_name(),
        "--group-by",
        "search",
        "--format",
        "json",
    ]);
    assert_snapshot!("search_json", search);
}
