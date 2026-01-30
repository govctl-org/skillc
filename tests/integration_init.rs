//! Integration tests for init command per RFC-0006.

mod common;

use common::{create_mock_home, run_skc_isolated};
use std::fs;
use tempfile::TempDir;

/// Test `skc init` creates project structure.
#[test]
fn test_init_project() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    let (stdout, _stderr, success) = run_skc_isolated(project_dir, &["init"], &[]);

    assert!(success, "init should succeed");
    assert!(
        stdout.contains("Initialized skillc project"),
        "should report initialization: {}",
        stdout
    );

    // Verify structure created
    assert!(
        project_dir.join(".skillc").exists(),
        ".skillc directory should exist"
    );
    assert!(
        project_dir.join(".skillc").join("skills").exists(),
        ".skillc/skills directory should exist"
    );
}

/// Test `skc init` is idempotent.
#[test]
fn test_init_project_idempotent() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // First init
    let (_, _, success1) = run_skc_isolated(project_dir, &["init"], &[]);
    assert!(success1, "first init should succeed");

    // Second init should also succeed (idempotent)
    let (stdout, _stderr, success2) = run_skc_isolated(project_dir, &["init"], &[]);
    assert!(success2, "second init should succeed (idempotent)");
    assert!(
        stdout.contains("Initialized skillc project"),
        "should report initialization: {}",
        stdout
    );
}

/// Test `skc init <name>` creates project-local skill.
#[test]
fn test_init_skill_local() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    let (stdout, _stderr, success) = run_skc_isolated(project_dir, &["init", "my-skill"], &[]);

    assert!(success, "init skill should succeed");
    assert!(
        stdout.contains("Created project skill 'my-skill'"),
        "should report skill creation: {}",
        stdout
    );

    // Verify skill created
    let skill_dir = project_dir.join(".skillc").join("skills").join("my-skill");
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
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // No prior init - should still work
    assert!(!project_dir.join(".skillc").exists());

    let (_, _, success) = run_skc_isolated(project_dir, &["init", "new-skill"], &[]);

    assert!(success, "init skill should succeed");
    assert!(
        project_dir.join(".skillc").exists(),
        ".skillc should be created"
    );
    assert!(
        project_dir.join(".skillc").join("skills").exists(),
        ".skillc/skills should be created"
    );
    assert!(
        project_dir
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
/// Uses HOME env var override to redirect global directory to temp.
#[cfg(unix)]
#[test]
fn test_init_skill_global() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // Create mock home directory structure
    let mock_home = create_mock_home(project_dir);

    // Use HOME env var to redirect ~/.skillc/skills/ to temp
    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["init", "global-skill", "--global"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "init global skill should succeed");
    assert!(
        stdout.contains("Created global skill 'global-skill'"),
        "should report global skill creation: {}",
        stdout
    );

    // Verify skill created in mock global location
    let skill_md = mock_home
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
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // First create the skill
    let (_, _, success1) = run_skc_isolated(project_dir, &["init", "existing-skill"], &[]);
    assert!(success1, "first init should succeed");

    // Second create should fail
    let (_stdout, stderr, success2) =
        run_skc_isolated(project_dir, &["init", "existing-skill"], &[]);

    assert!(!success2, "init existing skill should fail");
    assert!(
        stderr.contains("error[E050]"),
        "should return E050 error: {}",
        stderr
    );
    assert!(
        stderr.contains("skill 'existing-skill' already exists"),
        "should have clear message: {}",
        stderr
    );
}

/// Test skill name with hyphens is title-cased correctly.
#[test]
fn test_init_skill_title_case() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    let (_, _, success) = run_skc_isolated(project_dir, &["init", "my-cool-skill"], &[]);
    assert!(success, "init should succeed");

    let skill_md = project_dir
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
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    let (_, _, success) = run_skc_isolated(project_dir, &["init", "my_other_skill"], &[]);
    assert!(success, "init should succeed");

    let skill_md = project_dir
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
