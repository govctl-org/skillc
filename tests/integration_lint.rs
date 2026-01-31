//! Integration tests for lint command per RFC-0008.

mod common;

use common::TestContext;
use std::fs;

/// Test `skc lint` passes on valid skill.
#[test]
fn test_lint_valid_skill() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill_with_content(
        "valid-skill",
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
    );

    let result = ctx.run_skc(&["lint", "valid-skill"]);
    assert!(
        result.success,
        "lint should pass on valid skill. stdout: {}, stderr: {}",
        result.stdout, result.stderr
    );
}

/// Test `skc lint` fails on missing frontmatter fields.
#[test]
fn test_lint_missing_description() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill_with_content(
        "bad-skill",
        r#"---
name: bad-skill
---

# Bad Skill
"#,
    );

    let result = ctx.run_skc(&["lint", "bad-skill"]);
    result.assert_failure("lint on missing description");
    assert!(
        result.stderr.contains("SKL103") || result.stderr.contains("description"),
        "should report missing description: {}",
        result.stderr
    );
}

/// Test `skc lint` fails on missing name.
#[test]
fn test_lint_missing_name() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill_with_content(
        "no-name",
        r#"---
description: A skill without a name
---

# No Name
"#,
    );

    let result = ctx.run_skc(&["lint", "no-name"]);
    result.assert_failure("lint on missing name");
    assert!(
        result.stderr.contains("SKL102") || result.stderr.contains("name"),
        "should report missing name: {}",
        result.stderr
    );
}

/// Test `skc lint` warns on missing activation triggers.
#[test]
fn test_lint_missing_triggers() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("no-triggers");

    let result = ctx.run_skc(&["lint", "no-triggers"]);

    // Should have warning about triggers (may pass or fail depending on severity)
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("SKL107")
            || combined.contains("trigger")
            || combined.contains("activation"),
        "should mention triggers: {}",
        combined
    );
}

/// Test `skc lint` detects broken internal links.
#[test]
fn test_lint_broken_link() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill_with_content(
        "broken-links",
        r#"---
name: broken-links
description: A skill with broken links
---

# Broken Links

See [nonexistent](./does-not-exist.md) for more info.
"#,
    );

    let result = ctx.run_skc(&["lint", "broken-links"]);
    result.assert_failure("lint on broken links");
    assert!(
        result.stderr.contains("SKL301")
            || result.stderr.contains("broken")
            || result.stderr.contains("does-not-exist"),
        "should report broken link: {}",
        result.stderr
    );
}

/// Test `skc lint` with JSON output format.
#[test]
fn test_lint_json_output() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("json-test");

    let result = ctx.run_skc(&["lint", "json-test", "--format", "json"]);

    // Command should succeed or fail, but output should be valid JSON
    let output = if result.success {
        &result.stdout
    } else {
        &result.stderr
    };

    // Find the JSON part (may have other output)
    if let Some(start) = output.find('{') {
        let json_str = &output[start..];
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        assert!(
            parsed.is_ok(),
            "output should contain valid JSON: {}",
            output
        );
    } else if let Some(start) = output.find('[') {
        let json_str = &output[start..];
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_str);
        assert!(
            parsed.is_ok(),
            "output should contain valid JSON array: {}",
            output
        );
    }
}

/// Test `skc lint` on non-existent skill.
/// Uses SKILLC_HOME isolation because resolve_skill() searches global store.
#[test]
fn test_lint_skill_not_found() {
    let ctx = TestContext::new().with_project();
    ctx.ensure_global_skills_dir();

    let result = ctx.run_skc(&["lint", "nonexistent"]);
    result.assert_failure("lint on nonexistent skill");

    // E001 = skill not found, E010 = directory exists but missing SKILL.md
    assert!(
        result.stderr.contains("E001")
            || result.stderr.contains("E010")
            || result.stderr.contains("not found")
            || result.stderr.contains("missing SKILL.md"),
        "should report skill not found or invalid: {}",
        result.stderr
    );
}

/// Test `skc lint` detects name mismatch between frontmatter and directory.
#[test]
fn test_lint_name_mismatch() {
    let ctx = TestContext::new().with_project();

    // Create skill with mismatched name (directory is "dir-name" but frontmatter says "different-name")
    let skill_dir = ctx
        .project_dir()
        .join(".skillc")
        .join("skills")
        .join("dir-name");
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

    let result = ctx.run_skc(&["lint", "dir-name"]);

    // Name mismatch is a warning (SKL104), not an error, so lint may pass
    // Just check that the warning is reported
    let combined = format!("{}{}", result.stdout, result.stderr);
    assert!(
        combined.contains("SKL104")
            || combined.contains("mismatch")
            || combined.contains("different-name")
            || !result.success,
        "should report name mismatch or fail: {}",
        combined
    );
}
