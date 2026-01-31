//! Integration tests for list command per RFC-0007:C-LIST.

mod common;

use common::TestContext;

/// Test `skc list` with no skills returns empty.
#[test]
fn test_list_empty() {
    let ctx = TestContext::new().with_project();
    ctx.ensure_global_skills_dir();

    let result = ctx.run_skc(&["list"]);
    assert!(result.success, "list should succeed even with no skills");
    assert!(
        result.stdout.trim().is_empty()
            || result.stdout.contains("No skills found")
            || !result.stdout.contains("SKILL"),
        "should show no skills: {}",
        result.stdout
    );
}

/// Test `skc list` finds project-local skills.
#[test]
fn test_list_project_skills() {
    let ctx = TestContext::new().with_project();
    ctx.ensure_global_skills_dir();

    ctx.create_skill("project-skill-1");
    ctx.create_skill("project-skill-2");

    let result = ctx.run_skc(&["list"]);
    result.assert_success("list");

    assert!(
        result.stdout.contains("project-skill-1"),
        "should list project-skill-1: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("project-skill-2"),
        "should list project-skill-2: {}",
        result.stdout
    );
}

/// Test `skc list --scope project` only shows project skills.
#[test]
fn test_list_scope_project() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("local-skill");
    ctx.create_global_skill("global-skill");

    let result = ctx.run_skc(&["list", "--scope", "project"]);
    result.assert_success("list --scope project");

    assert!(
        result.stdout.contains("local-skill"),
        "should list local-skill: {}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("global-skill"),
        "should NOT list global-skill: {}",
        result.stdout
    );
}

/// Test `skc list --scope global` only shows global skills.
#[test]
fn test_list_scope_global() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("local-skill");
    ctx.create_global_skill("global-skill");

    let result = ctx.run_skc(&["list", "--scope", "global"]);
    result.assert_success("list --scope global");

    assert!(
        result.stdout.contains("global-skill"),
        "should list global-skill: {}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("local-skill"),
        "should NOT list local-skill: {}",
        result.stdout
    );
}

/// Test `skc list --format json` outputs valid JSON.
#[test]
fn test_list_json_output() {
    let ctx = TestContext::new().with_project();
    ctx.ensure_global_skills_dir();
    ctx.create_skill("json-skill");

    let result = ctx.run_skc(&["list", "--format", "json"]);
    result.assert_success("list --format json");

    // Should be valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result.stdout);
    assert!(
        parsed.is_ok(),
        "output should be valid JSON: {}",
        result.stdout
    );

    let json = parsed.expect("should be valid JSON");
    assert!(json.get("skills").is_some(), "should have skills array");
}

/// Test `skc list --status not-built` filters correctly.
#[test]
fn test_list_status_not_built() {
    let ctx = TestContext::new().with_project();
    ctx.ensure_global_skills_dir();
    ctx.create_skill("unbuilt-skill");

    let result = ctx.run_skc(&["list", "--status", "not-built"]);
    result.assert_success("list --status not-built");

    assert!(
        result.stdout.contains("unbuilt-skill"),
        "should list unbuilt skill: {}",
        result.stdout
    );
}

/// Test `skc list --pattern` filters by name.
#[test]
fn test_list_pattern_filter() {
    let ctx = TestContext::new().with_project();
    ctx.ensure_global_skills_dir();

    ctx.create_skill("rust-skill");
    ctx.create_skill("python-skill");
    ctx.create_skill("rust-advanced");

    let result = ctx.run_skc(&["list", "--pattern", "rust*"]);
    result.assert_success("list --pattern");

    assert!(
        result.stdout.contains("rust-skill"),
        "should list rust-skill: {}",
        result.stdout
    );
    assert!(
        result.stdout.contains("rust-advanced"),
        "should list rust-advanced: {}",
        result.stdout
    );
    assert!(
        !result.stdout.contains("python-skill"),
        "should NOT list python-skill: {}",
        result.stdout
    );
}

/// Test `skc list` shows skill scope (project vs global).
#[test]
fn test_list_shows_scope() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("local-skill");
    ctx.create_global_skill("global-skill");

    let result = ctx.run_skc(&["list"]);
    result.assert_success("list");

    // Should indicate scope somehow (text varies by implementation)
    assert!(
        result.stdout.contains("project")
            || result.stdout.contains("global")
            || result.stdout.contains("local"),
        "should indicate scope: {}",
        result.stdout
    );
}
