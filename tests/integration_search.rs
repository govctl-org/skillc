//! CLI integration tests for search command per RFC-0004.

mod common;

use common::TestContext;
use insta::assert_snapshot;

/// Build the skill to create search index.
fn build_skill(ctx: &TestContext) {
    let result = ctx.run_skc(&["build", ctx.skill_name(), "--target", ctx.mock_agent_str()]);
    result.assert_success("build skill for search");
}

#[test]
fn test_search_text_output() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);

    let output = ctx.run_skc_text(&["search", ctx.skill_name(), "performance tips"]);
    assert_snapshot!("search_text", output);
}

#[test]
fn test_search_json_output() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);

    let output = ctx.run_skc_json(&[
        "search",
        ctx.skill_name(),
        "performance",
        "--format",
        "json",
    ]);
    assert_snapshot!("search_json", output);
}

#[test]
fn test_search_no_results() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);

    let output = ctx.run_skc_json(&[
        "search",
        ctx.skill_name(),
        "nonexistent_query_xyz123",
        "--format",
        "json",
    ]);
    assert_snapshot!("search_no_results", output);
}

#[test]
fn test_search_limit() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);

    let output = ctx.run_skc_json(&[
        "search",
        ctx.skill_name(),
        "test",
        "--format",
        "json",
        "--limit",
        "2",
    ]);
    assert_snapshot!("search_limit", output);
}
