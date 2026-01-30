//! Integration tests for lint command per RFC-0008.

mod common;

#[cfg(unix)]
use common::{create_mock_home, create_project_skill, run_skc_isolated};
#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use tempfile::TempDir;

/// Test `skc lint` passes on valid skill.
#[cfg(unix)]
#[test]
fn test_lint_valid_skill() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    create_project_skill(project_dir, "valid-skill");

    // Add activation trigger to make it fully valid
    let skill_md = project_dir
        .join(".skillc")
        .join("skills")
        .join("valid-skill")
        .join("SKILL.md");
    fs::write(
        &skill_md,
        r#"---
name: valid-skill
description: A valid test skill for linting
---

# Valid Skill

## When to use this skill

Use this skill when testing lint functionality.

## Getting Started

Instructions here.
"#,
    )
    .expect("failed to write SKILL.md");

    let (stdout, stderr, success) = run_skc_isolated(
        project_dir,
        &["lint", "valid-skill"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(
        success,
        "lint should pass on valid skill. stdout: {}, stderr: {}",
        stdout, stderr
    );
}

/// Test `skc lint` fails on missing frontmatter fields.
#[cfg(unix)]
#[test]
fn test_lint_missing_description() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    // Create skill with missing description
    let skillc_dir = project_dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");

    let skill_dir = skillc_dir.join("skills").join("bad-skill");
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: bad-skill
---

# Bad Skill
"#,
    )
    .expect("failed to write SKILL.md");

    let (_stdout, stderr, success) = run_skc_isolated(
        project_dir,
        &["lint", "bad-skill"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(!success, "lint should fail on missing description");
    assert!(
        stderr.contains("SKL103") || stderr.contains("description"),
        "should report missing description: {}",
        stderr
    );
}

/// Test `skc lint` fails on missing name.
#[cfg(unix)]
#[test]
fn test_lint_missing_name() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    // Create skill with missing name
    let skillc_dir = project_dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");

    let skill_dir = skillc_dir.join("skills").join("no-name");
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
description: A skill without a name
---

# No Name
"#,
    )
    .expect("failed to write SKILL.md");

    let (_stdout, stderr, success) = run_skc_isolated(
        project_dir,
        &["lint", "no-name"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(!success, "lint should fail on missing name");
    assert!(
        stderr.contains("SKL102") || stderr.contains("name"),
        "should report missing name: {}",
        stderr
    );
}

/// Test `skc lint` warns on missing activation triggers.
#[cfg(unix)]
#[test]
fn test_lint_missing_triggers() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    create_project_skill(project_dir, "no-triggers");

    let (stdout, stderr, _success) = run_skc_isolated(
        project_dir,
        &["lint", "no-triggers"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    // Should have warning about triggers (may pass or fail depending on severity)
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("SKL107")
            || combined.contains("trigger")
            || combined.contains("activation"),
        "should mention triggers: {}",
        combined
    );
}

/// Test `skc lint` detects broken internal links.
#[cfg(unix)]
#[test]
fn test_lint_broken_link() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    // Create skill with broken link
    let skillc_dir = project_dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");

    let skill_dir = skillc_dir.join("skills").join("broken-links");
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: broken-links
description: A skill with broken links
---

# Broken Links

See [nonexistent](./does-not-exist.md) for more info.
"#,
    )
    .expect("failed to write SKILL.md");

    let (_stdout, stderr, success) = run_skc_isolated(
        project_dir,
        &["lint", "broken-links"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(!success, "lint should fail on broken links");
    assert!(
        stderr.contains("SKL301") || stderr.contains("broken") || stderr.contains("does-not-exist"),
        "should report broken link: {}",
        stderr
    );
}

/// Test `skc lint` with JSON output format.
#[cfg(unix)]
#[test]
fn test_lint_json_output() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    create_project_skill(project_dir, "json-test");

    let (stdout, stderr, success) = run_skc_isolated(
        project_dir,
        &["lint", "json-test", "--format", "json"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    // Command should succeed or fail, but output should be valid JSON
    let output = if success { &stdout } else { &stderr };

    // Find the JSON part (may have other output)
    if let Some(start) = output.find('{') {
        let json_str = &output[start..];
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        assert!(
            parsed.is_ok(),
            "output should contain valid JSON: {}",
            output
        );
    } else {
        // If no JSON object, check for JSON array
        if let Some(start) = output.find('[') {
            let json_str = &output[start..];
            let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
            assert!(
                parsed.is_ok(),
                "output should contain valid JSON array: {}",
                output
            );
        }
    }
}

/// Test `skc lint` on non-existent skill.
#[cfg(unix)]
#[test]
fn test_lint_skill_not_found() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    // Create project structure but no skill
    let skillc_dir = project_dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");
    fs::create_dir_all(skillc_dir.join("skills")).expect("failed to create skills dir");

    // Also create empty global skills dir
    fs::create_dir_all(mock_home.join(".skillc").join("skills"))
        .expect("failed to create global skills dir");

    let (_stdout, stderr, success) = run_skc_isolated(
        project_dir,
        &["lint", "nonexistent"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    assert!(!success, "lint should fail on nonexistent skill");
    // E001 = skill not found, E010 = directory exists but missing SKILL.md
    assert!(
        stderr.contains("E001")
            || stderr.contains("E010")
            || stderr.contains("not found")
            || stderr.contains("missing SKILL.md"),
        "should report skill not found or invalid: {}",
        stderr
    );
}

/// Test `skc lint` detects name mismatch between frontmatter and directory.
#[cfg(unix)]
#[test]
fn test_lint_name_mismatch() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let project_dir = temp.path();
    let mock_home = create_mock_home(project_dir);

    // Create skill with mismatched name
    let skillc_dir = project_dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");

    // Directory name is "dir-name" but frontmatter says "different-name"
    let skill_dir = skillc_dir.join("skills").join("dir-name");
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: different-name
description: Name doesn't match directory
---

# Different Name
"#,
    )
    .expect("failed to write SKILL.md");

    let (stdout, stderr, success) = run_skc_isolated(
        project_dir,
        &["lint", "dir-name"],
        &[("HOME", mock_home.to_str().expect("path is UTF-8"))],
    );

    // Name mismatch is a warning (SKL104), not an error, so lint may pass
    // Just check that the warning is reported
    let combined = format!("{}{}", stdout, stderr);
    assert!(
        combined.contains("SKL104")
            || combined.contains("mismatch")
            || combined.contains("different-name")
            || !success,
        "should report name mismatch or fail: {}",
        combined
    );
}
