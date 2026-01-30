//! Common helpers for CLI integration tests.
//!
//! Note: Rust compiles each integration test file as a separate crate.
//! Functions may appear "unused" in one test file but are used by others.
//! This is expected behavior, not actual dead code.

// Allow dead_code warnings since functions are used across different test crates
#![allow(dead_code)]

use assert_cmd::Command;
use regex::Regex;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

pub struct TestWorkspace {
    pub dir: TempDir,
    pub skill_dir: PathBuf,
    #[allow(dead_code)]
    pub skill_name: String,
}

pub fn init_workspace() -> TestWorkspace {
    let dir = TempDir::new().expect("failed to create temp dir");
    let root = dir.path();

    let skill_name = "test-skill".to_string();

    // Create project structure with skill inside .skillc/skills/
    let skillc_dir = root.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");

    let skill_dir = skillc_dir.join("skills").join(&skill_name);
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");

    write_skill(&skill_dir);

    TestWorkspace {
        dir,
        skill_dir,
        skill_name,
    }
}

fn write_skill(skill_dir: &Path) {
    fs::create_dir_all(skill_dir.join("docs")).expect("failed to create docs dir");

    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: test-skill
description: A test skill
---

# Test Skill

## Getting Started

Intro text.

### Prerequisites

You need these things.

## API Reference

API docs here.
"#,
    )
    .expect("failed to write SKILL.md");

    fs::write(
        skill_dir.join("docs").join("advanced.md"),
        r#"# Advanced Topics

## Performance

Performance tips here.
"#,
    )
    .expect("failed to write advanced.md");
}

pub fn run_skc(dir: &Path, args: &[&str]) -> String {
    let output = run_skc_output(dir, args);
    if !output.status.success() {
        panic!(
            "skc {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    normalize_text(&String::from_utf8_lossy(&output.stdout), dir)
}

#[allow(dead_code)]
pub fn run_skc_allow_fail(dir: &Path, args: &[&str]) -> String {
    let output = run_skc_output(dir, args);
    normalize_text(&String::from_utf8_lossy(&output.stdout), dir)
}

#[allow(dead_code)]
pub fn run_skc_json(dir: &Path, args: &[&str]) -> String {
    let output = run_skc_output(dir, args);
    if !output.status.success() {
        panic!(
            "skc {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    normalize_json(&String::from_utf8_lossy(&output.stdout), dir)
}

fn run_skc_output(dir: &Path, args: &[&str]) -> std::process::Output {
    run_skc_output_with_env(dir, args, &[])
}

fn run_skc_output_with_env(
    dir: &Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> std::process::Output {
    let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("skc"));
    cmd.args(args)
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .env("SKC_RUN_ID", "TEST-RUN-ID");

    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    cmd.output().expect("failed to run skc")
}

/// Run skc with environment overrides for isolated testing.
/// Returns (stdout, stderr, success).
pub fn run_skc_isolated(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (String, String, bool) {
    let output = run_skc_output_with_env(dir, args, env);
    (
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
        output.status.success(),
    )
}

/// Create a minimal skill directory with SKILL.md.
pub fn create_minimal_skill(dir: &Path, name: &str) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {}\ndescription: test\n---\n# {}\n", name, name),
    )
    .expect("failed to write SKILL.md");
}

/// Create a minimal skill in a project structure (`.skillc/skills/<name>/`).
/// Returns the project root directory.
pub fn create_project_skill(project_dir: &Path, name: &str) {
    let skillc_dir = project_dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");

    let skill_dir = skillc_dir.join("skills").join(name);
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {}\ndescription: test\n---\n# {}\n", name, name),
    )
    .expect("failed to write SKILL.md");
}

/// Create a test skill with triggers (for build tests).
pub fn create_test_skill(dir: &Path, name: &str) -> PathBuf {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        format!(
            "---\nname: {}\ndescription: \"Test skill for {}\"\ntriggers:\n  - test\n---\n\n# {}\n\nTest skill.\n",
            name, name, name
        ),
    )
    .expect("failed to write SKILL.md");
    skill_dir
}

/// Create a project structure with .skillc directory.
pub fn create_project(dir: &Path) -> PathBuf {
    let skillc_dir = dir.join(".skillc");
    fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
    fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");
    dir.to_path_buf()
}

/// Get the path to the fallback logs database for a skill.
pub fn fallback_db_path(project_dir: &Path, skill_name: &str) -> PathBuf {
    project_dir
        .join(".skillc")
        .join("logs")
        .join(skill_name)
        .join(".skillc-meta")
        .join("logs.db")
}

/// Create a mock home directory inside project_dir for isolated testing.
/// Returns the mock home path.
pub fn create_mock_home(project_dir: &Path) -> PathBuf {
    let mock_home = project_dir.join("mock_home");
    fs::create_dir_all(&mock_home).expect("failed to create mock home");
    mock_home
}

/// Get the path to the runtime logs database for a skill in a mock home.
/// Uses the SSOT runtime location (~/.skillc/runtime/<skill>/)
pub fn runtime_db_path(mock_home: &Path, skill_name: &str) -> PathBuf {
    mock_home
        .join(".skillc")
        .join("runtime")
        .join(skill_name)
        .join(".skillc-meta")
        .join("logs.db")
}

pub fn normalize_text(output: &str, dir: &Path) -> String {
    let dir_str = dir.display().to_string();
    let mut normalized = output.replace(&format!("/private{}", dir_str), "<TEMPDIR>");
    normalized = normalized.replace(&dir_str, "<TEMPDIR>");

    let ts_pattern =
        Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})")
            .expect("valid timestamp regex");
    normalized = ts_pattern.replace_all(&normalized, "<TS>").to_string();

    normalized.trim_end().to_string()
}

#[allow(dead_code)]
pub fn normalize_json(output: &str, dir: &Path) -> String {
    let mut value: Value = serde_json::from_str(output).expect("output should be valid JSON");
    normalize_value(&mut value, dir);
    let rendered = serde_json::to_string_pretty(&value).expect("json render failed");
    rendered.trim_end().to_string()
}

#[allow(dead_code)]
fn normalize_value(value: &mut Value, dir: &Path) {
    match value {
        Value::String(s) => {
            let dir_str = dir.display().to_string();
            *s = s.replace(&format!("/private{}", dir_str), "<TEMPDIR>");
            *s = s.replace(&dir_str, "<TEMPDIR>");
            let ts_pattern =
                Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})")
                    .expect("valid timestamp regex");
            if ts_pattern.is_match(s) {
                *s = "<TS>".to_string();
            }
        }
        Value::Number(n) => {
            // Round floating point scores to 2 decimal places for stability
            // Only if it's actually a float (not an integer)
            if !n.is_i64()
                && !n.is_u64()
                && let Some(f) = n.as_f64()
            {
                *value = Value::Number(
                    serde_json::Number::from_f64((f * 100.0).round() / 100.0)
                        .unwrap_or_else(|| serde_json::Number::from(0)),
                );
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_value(item, dir);
            }
        }
        Value::Object(map) => {
            let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (key, mut value) in entries {
                normalize_value(&mut value, dir);
                map.insert(key, value);
            }
        }
        _ => {}
    }
}
