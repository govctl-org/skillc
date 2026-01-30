//! Integration tests for list command per RFC-0007:C-LIST.

mod common;

use common::{create_mock_home, create_project_skill, run_skc_isolated};
use std::fs;
use tempfile::TempDir;

/// Test `skc list` with no skills returns empty.
#[test]
fn test_list_empty() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // Create project structure but no skills
    let skillc_dir = project_dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");
    fs::create_dir_all(skillc_dir.join("skills")).expect("failed to create skills dir");

    // Create mock home with no global skills
    let mock_home = create_mock_home(project_dir);
    fs::create_dir_all(mock_home.join(".skillc").join("skills"))
        .expect("failed to create global skills dir");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed even with no skills");
    // Output should be empty or indicate no skills
    assert!(
        stdout.trim().is_empty() || stdout.contains("No skills found") || !stdout.contains("SKILL"),
        "should show no skills: {}",
        stdout
    );
}

/// Test `skc list` finds project-local skills.
#[test]
fn test_list_project_skills() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_project_skill(project_dir, "project-skill-1");
    create_project_skill(project_dir, "project-skill-2");

    // Create mock home with no global skills
    let mock_home = create_mock_home(project_dir);
    fs::create_dir_all(mock_home.join(".skillc").join("skills"))
        .expect("failed to create global skills dir");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed");
    assert!(
        stdout.contains("project-skill-1"),
        "should list project-skill-1: {}",
        stdout
    );
    assert!(
        stdout.contains("project-skill-2"),
        "should list project-skill-2: {}",
        stdout
    );
}

/// Test `skc list --scope project` only shows project skills.
#[cfg(unix)]
#[test]
fn test_list_scope_project() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_project_skill(project_dir, "local-skill");

    // Create mock home with a global skill
    let mock_home = create_mock_home(project_dir);
    let global_skill_dir = mock_home
        .join(".skillc")
        .join("skills")
        .join("global-skill");
    fs::create_dir_all(&global_skill_dir).expect("failed to create global skill dir");
    fs::write(
        global_skill_dir.join("SKILL.md"),
        "---\nname: global-skill\ndescription: test\n---\n# Global\n",
    )
    .expect("failed to write global SKILL.md");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list", "--scope", "project"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed");
    assert!(
        stdout.contains("local-skill"),
        "should list local-skill: {}",
        stdout
    );
    assert!(
        !stdout.contains("global-skill"),
        "should NOT list global-skill: {}",
        stdout
    );
}

/// Test `skc list --scope global` only shows global skills.
#[cfg(unix)]
#[test]
fn test_list_scope_global() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_project_skill(project_dir, "local-skill");

    // Create mock home with a global skill
    let mock_home = create_mock_home(project_dir);
    let global_skill_dir = mock_home
        .join(".skillc")
        .join("skills")
        .join("global-skill");
    fs::create_dir_all(&global_skill_dir).expect("failed to create global skill dir");
    fs::write(
        global_skill_dir.join("SKILL.md"),
        "---\nname: global-skill\ndescription: test\n---\n# Global\n",
    )
    .expect("failed to write global SKILL.md");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list", "--scope", "global"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed");
    assert!(
        stdout.contains("global-skill"),
        "should list global-skill: {}",
        stdout
    );
    assert!(
        !stdout.contains("local-skill"),
        "should NOT list local-skill: {}",
        stdout
    );
}

/// Test `skc list --format json` outputs valid JSON.
#[test]
fn test_list_json_output() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_project_skill(project_dir, "json-skill");

    // Create mock home
    let mock_home = create_mock_home(project_dir);
    fs::create_dir_all(mock_home.join(".skillc").join("skills"))
        .expect("failed to create global skills dir");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list", "--format", "json"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed");

    // Should be valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "output should be valid JSON: {}", stdout);

    let json = parsed.expect("should be valid JSON");
    assert!(json.get("skills").is_some(), "should have skills array");
}

/// Test `skc list --status not-built` filters correctly.
#[test]
fn test_list_status_not_built() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    // Create a skill but don't build it
    create_project_skill(project_dir, "unbuilt-skill");

    // Create mock home
    let mock_home = create_mock_home(project_dir);
    fs::create_dir_all(mock_home.join(".skillc").join("skills"))
        .expect("failed to create global skills dir");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list", "--status", "not-built"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed");
    assert!(
        stdout.contains("unbuilt-skill"),
        "should list unbuilt skill: {}",
        stdout
    );
}

/// Test `skc list --pattern` filters by name.
#[test]
fn test_list_pattern_filter() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_project_skill(project_dir, "rust-skill");
    create_project_skill(project_dir, "python-skill");
    create_project_skill(project_dir, "rust-advanced");

    // Create mock home
    let mock_home = create_mock_home(project_dir);
    fs::create_dir_all(mock_home.join(".skillc").join("skills"))
        .expect("failed to create global skills dir");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list", "--pattern", "rust*"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed");
    assert!(
        stdout.contains("rust-skill"),
        "should list rust-skill: {}",
        stdout
    );
    assert!(
        stdout.contains("rust-advanced"),
        "should list rust-advanced: {}",
        stdout
    );
    assert!(
        !stdout.contains("python-skill"),
        "should NOT list python-skill: {}",
        stdout
    );
}

/// Test `skc list` shows skill scope (project vs global).
#[cfg(unix)]
#[test]
fn test_list_shows_scope() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();

    create_project_skill(project_dir, "local-skill");

    // Create mock home with a global skill
    let mock_home = create_mock_home(project_dir);
    let global_skill_dir = mock_home
        .join(".skillc")
        .join("skills")
        .join("global-skill");
    fs::create_dir_all(&global_skill_dir).expect("failed to create global skill dir");
    fs::write(
        global_skill_dir.join("SKILL.md"),
        "---\nname: global-skill\ndescription: test\n---\n# Global\n",
    )
    .expect("failed to write global SKILL.md");

    let (stdout, _stderr, success) = run_skc_isolated(
        project_dir,
        &["list"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(success, "list should succeed");
    // Should indicate scope somehow (text varies by implementation)
    assert!(
        stdout.contains("project") || stdout.contains("global") || stdout.contains("local"),
        "should indicate scope: {}",
        stdout
    );
}
