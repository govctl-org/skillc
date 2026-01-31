//! Integration tests for init command per RFC-0006.

mod common;

use common::TestContext;
use std::fs;

/// Test `skc init` creates project structure.
#[test]
fn test_init_project() {
    let ctx = TestContext::new();

    let result = ctx.run_skc(&["init"]);
    result.assert_success("init");

    assert!(
        result.stdout.contains("Initialized skillc project"),
        "should report initialization: {}",
        result.stdout
    );

    // Verify structure created
    assert!(
        ctx.temp_path().join(".skillc").exists(),
        ".skillc directory should exist"
    );
    assert!(
        ctx.temp_path().join(".skillc").join("skills").exists(),
        ".skillc/skills directory should exist"
    );
}

/// Test `skc init` is idempotent.
#[test]
fn test_init_project_idempotent() {
    let ctx = TestContext::new();

    // First init
    let result1 = ctx.run_skc(&["init"]);
    result1.assert_success("first init");

    // Second init should also succeed (idempotent)
    let result2 = ctx.run_skc(&["init"]);
    result2.assert_success("second init");
    assert!(
        result2.stdout.contains("Initialized skillc project"),
        "should report initialization: {}",
        result2.stdout
    );
}

/// Test `skc init <name>` creates project-local skill.
#[test]
fn test_init_skill_local() {
    let ctx = TestContext::new();

    let result = ctx.run_skc(&["init", "my-skill"]);
    result.assert_success("init skill");

    assert!(
        result.stdout.contains("Created project skill 'my-skill'"),
        "should report skill creation: {}",
        result.stdout
    );

    // Verify skill created
    let skill_dir = ctx
        .temp_path()
        .join(".skillc")
        .join("skills")
        .join("my-skill");
    assert!(skill_dir.exists(), "skill directory should exist");

    let skill_md = skill_dir.join("SKILL.md");
    assert!(skill_md.exists(), "SKILL.md should exist");

    // Verify SKILL.md content
    let content = fs::read_to_string(&skill_md).expect("failed to read SKILL.md");
    assert!(
        content.contains("name: my-skill"),
        "should have correct name"
    );
    assert!(
        content.contains("description: \"TODO: Add skill description\""),
        "should have placeholder description"
    );
    assert!(
        content.contains("# My Skill"),
        "should have title-cased heading"
    );
}

/// Test `skc init <name>` creates project structure if not exists.
#[test]
fn test_init_skill_creates_project_structure() {
    let ctx = TestContext::new();

    // No prior init - should still work
    assert!(!ctx.temp_path().join(".skillc").exists());

    let result = ctx.run_skc(&["init", "new-skill"]);
    result.assert_success("init skill without prior init");

    assert!(
        ctx.temp_path().join(".skillc").exists(),
        ".skillc should be created"
    );
    assert!(
        ctx.temp_path().join(".skillc").join("skills").exists(),
        ".skillc/skills should be created"
    );
    assert!(
        ctx.temp_path()
            .join(".skillc")
            .join("skills")
            .join("new-skill")
            .join("SKILL.md")
            .exists(),
        "skill SKILL.md should exist"
    );
}

/// Test `skc init <name> --global` creates global skill.
///
/// Uses SKILLC_HOME env var override to redirect global directory to temp.
#[test]
fn test_init_skill_global() {
    let ctx = TestContext::new();

    let result = ctx.run_skc(&["init", "global-skill", "--global"]);
    result.assert_success("init global skill");

    assert!(
        result
            .stdout
            .contains("Created global skill 'global-skill'"),
        "should report global skill creation: {}",
        result.stdout
    );

    // Verify skill created in mock global location
    let skill_md = ctx
        .mock_home()
        .join(".skillc")
        .join("skills")
        .join("global-skill")
        .join("SKILL.md");
    assert!(
        skill_md.exists(),
        "global SKILL.md should exist at {}",
        skill_md.display()
    );
}

/// Test error when skill already exists.
#[test]
fn test_init_skill_already_exists() {
    let ctx = TestContext::new();

    // First create the skill
    let result1 = ctx.run_skc(&["init", "existing-skill"]);
    result1.assert_success("first init");

    // Second create should fail
    let result2 = ctx.run_skc(&["init", "existing-skill"]);
    result2.assert_failure("init existing skill");

    assert!(
        result2.stderr.contains("error[E050]"),
        "should return E050 error: {}",
        result2.stderr
    );
    assert!(
        result2
            .stderr
            .contains("skill 'existing-skill' already exists"),
        "should have clear message: {}",
        result2.stderr
    );
}

/// Test skill name with hyphens is title-cased correctly.
#[test]
fn test_init_skill_title_case() {
    let ctx = TestContext::new();

    let result = ctx.run_skc(&["init", "my-cool-skill"]);
    result.assert_success("init");

    let skill_md = ctx
        .temp_path()
        .join(".skillc")
        .join("skills")
        .join("my-cool-skill")
        .join("SKILL.md");
    let content = fs::read_to_string(&skill_md).expect("failed to read SKILL.md");

    assert!(
        content.contains("# My Cool Skill"),
        "should title-case with spaces: {}",
        content
    );
}

/// Test skill name with underscores is title-cased correctly.
#[test]
fn test_init_skill_title_case_underscore() {
    let ctx = TestContext::new();

    let result = ctx.run_skc(&["init", "my_other_skill"]);
    result.assert_success("init");

    let skill_md = ctx
        .temp_path()
        .join(".skillc")
        .join("skills")
        .join("my_other_skill")
        .join("SKILL.md");
    let content = fs::read_to_string(&skill_md).expect("failed to read SKILL.md");

    assert!(
        content.contains("# My Other Skill"),
        "should title-case underscores: {}",
        content
    );
}
