//! CLI integration tests for gateway commands.

mod common;

use common::TestContext;
use insta::assert_snapshot;

/// Build the skill to create index per [[RFC-0004:C-INDEX]].
fn build_skill(ctx: &TestContext) {
    let result = ctx.run_skc(&["build", ctx.skill_name(), "--target", ctx.mock_agent_str()]);
    result.assert_success("build skill for gateway");
}

/// Test outline command output matches snapshot (unbuilt skill, filesystem fallback)
#[test]
fn test_outline_snapshot() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let output = ctx.run_skc_text(&["outline", ctx.skill_name()]);
    assert_snapshot!("outline", output);
}

/// Test outline --level filters headings by level per [[RFC-0002:C-OUTLINE]] (unbuilt)
#[test]
fn test_outline_level_filter() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    // --level 2 should only show # and ## headings, not ### (Prerequisites)
    let output = ctx.run_skc_text(&["outline", ctx.skill_name(), "--level", "2"]);
    assert_snapshot!("outline_level_2", output);
}

/// Test outline uses pre-built index when available per [[RFC-0002:C-OUTLINE]]
#[test]
fn test_outline_with_index() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);

    // Should produce same output as unbuilt, but use the index internally
    let output = ctx.run_skc_text(&["outline", ctx.skill_name()]);
    assert_snapshot!("outline_with_index", output);
}

/// Test outline --level with pre-built index
#[test]
fn test_outline_level_filter_with_index() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);

    let output = ctx.run_skc_text(&["outline", ctx.skill_name(), "--level", "2"]);
    assert_snapshot!("outline_level_2_with_index", output);
}

/// Test show command output matches snapshot per [[RFC-0002:C-SHOW]]
/// Uses fallback runtime parsing when index is not available.
#[test]
fn test_show_snapshot() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let output = ctx.run_skc_text(&["show", ctx.skill_name(), "--section", "Getting Started"]);
    assert_snapshot!("show", output);
}

/// Test show --max-lines truncates output per [[RFC-0002:C-SHOW]]
/// Uses fallback runtime parsing when index is not available.
#[test]
fn test_show_max_lines() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    // Getting Started section has multiple lines, limit to 3
    let output = ctx.run_skc_text(&[
        "show",
        ctx.skill_name(),
        "--section",
        "Getting Started",
        "--max-lines",
        "3",
    ]);
    assert_snapshot!("show_max_lines", output);
}

/// Test open command output matches snapshot
#[test]
fn test_open_snapshot() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let output = ctx.run_skc_text(&["open", ctx.skill_name(), "docs/advanced.md"]);
    assert_snapshot!("open", output);
}

/// Test open --max-lines truncates output per [[RFC-0002:C-OPEN]]
#[test]
fn test_open_max_lines() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    // Limit output to 3 lines
    let output = ctx.run_skc_text(&[
        "open",
        ctx.skill_name(),
        "docs/advanced.md",
        "--max-lines",
        "3",
    ]);
    assert_snapshot!("open_max_lines", output);
}

// === Index-based show tests per [[RFC-0002:C-SHOW]] ===

/// Test show with built index uses index-based lookup per [[RFC-0002:C-SHOW]]
#[test]
fn test_show_with_index() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);
    let output = ctx.run_skc_text(&["show", ctx.skill_name(), "--section", "Getting Started"]);
    assert_snapshot!("show_with_index", output);
}

/// Test show strips em-dash suffix from query per [[RFC-0002:C-SHOW]]
#[test]
fn test_show_emdash_stripping() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);
    // Query with description suffix (as it might appear in compiled stub)
    let output = ctx.run_skc_text(&[
        "show",
        ctx.skill_name(),
        "--section",
        "Getting Started — How to begin using the skill",
    ]);
    // Should still find "Getting Started" after stripping the em-dash suffix
    assert_snapshot!("show_emdash_stripping", output);
}

/// Test show provides suggestions on section not found per [[RFC-0002:C-SHOW]]
#[test]
fn test_show_suggestions() {
    let ctx = TestContext::new()
        .with_rich_skill("test-skill")
        .with_mock_agent();
    build_skill(&ctx);
    // Query for non-existent section with similar name
    let result = ctx.run_skc(&["show", ctx.skill_name(), "--section", "Getting"]);
    // Should fail but provide suggestions
    assert!(!result.success);
    assert!(result.stderr.contains("section not found"));
    assert!(result.stderr.contains("Did you mean"));
    assert!(result.stderr.contains("Getting Started"));
}

// === Sources command tests per [[RFC-0002:C-SOURCES]] ===

/// Test sources command text output (tree format) per [[RFC-0002:C-SOURCES]]
#[test]
fn test_sources_snapshot() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let output = ctx.run_skc_text(&["sources", ctx.skill_name()]);
    assert_snapshot!("sources", output);
}

/// Test sources command JSON output per [[RFC-0002:C-SOURCES]]
#[test]
fn test_sources_json() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let output = ctx.run_skc_json(&["sources", ctx.skill_name(), "--format", "json"]);
    assert_snapshot!("sources_json", output);
}

/// Test sources --depth limits directory traversal per [[RFC-0002:C-SOURCES]]
#[test]
fn test_sources_depth() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    // --depth 1 should only show top-level entries, not contents of docs/
    let output = ctx.run_skc_text(&["sources", ctx.skill_name(), "--depth", "1"]);
    assert_snapshot!("sources_depth_1", output);
}

/// Test sources --dir filters to subdirectory per [[RFC-0002:C-SOURCES]]
#[test]
fn test_sources_subdir() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let output = ctx.run_skc_text(&["sources", ctx.skill_name(), "--dir", "docs"]);
    assert_snapshot!("sources_subdir", output);
}

/// Test sources --pattern filters by glob per [[RFC-0002:C-SOURCES]]
#[test]
fn test_sources_pattern() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    // Only show .md files
    let output = ctx.run_skc_text(&["sources", ctx.skill_name(), "--pattern", "*.md"]);
    assert_snapshot!("sources_pattern", output);
}

/// Test sources --limit truncates output per [[RFC-0002:C-SOURCES]]
#[test]
fn test_sources_limit() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let output = ctx.run_skc_text(&["sources", ctx.skill_name(), "--limit", "2"]);
    assert_snapshot!("sources_limit", output);
}

/// Test sources rejects path traversal per [[RFC-0002:C-SOURCES]]
#[test]
fn test_sources_path_escape() {
    let ctx = TestContext::new().with_rich_skill("test-skill");
    let result = ctx.run_skc(&["sources", ctx.skill_name(), "--dir", "../.."]);
    assert!(!result.success);
    assert!(result.stderr.contains("path escapes") || result.stderr.contains("not found"));
}
