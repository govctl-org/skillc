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

// ============================================================================
// TestContext - Unified test setup and execution
// ============================================================================

/// Test execution result with stdout, stderr, and success status.
pub struct TestResult {
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

impl TestResult {
    /// Assert the command succeeded, panic with output if not.
    pub fn assert_success(&self, context: &str) {
        assert!(
            self.success,
            "{} failed: stdout={}, stderr={}",
            context, self.stdout, self.stderr
        );
    }

    /// Assert the command failed.
    pub fn assert_failure(&self, context: &str) {
        assert!(
            !self.success,
            "{} should have failed but succeeded: stdout={}",
            context, self.stdout
        );
    }
}

/// Unified test context for integration tests.
///
/// Provides isolated temp directory, project setup, mock home, and mock agent.
/// All paths are guaranteed to exist when accessed.
///
/// Per [[RFC-0009:C-ENV-OVERRIDE]], all tests are automatically isolated via
/// `SKILLC_HOME` environment variable - no real home directory is ever accessed.
pub struct TestContext {
    temp: TempDir,
    project_dir: Option<PathBuf>,
    mock_home: PathBuf,
    mock_agent: Option<PathBuf>,
    rich_skill_name: Option<String>,
}

impl TestContext {
    /// Create a new test context with isolated SKILLC_HOME.
    ///
    /// All skc commands run through this context will use the mock home
    /// directory, ensuring complete isolation from the real home directory.
    pub fn new() -> Self {
        let temp = TempDir::new().expect("failed to create temp dir");
        let mock_home = temp.path().join("mock_home");
        fs::create_dir_all(&mock_home).expect("failed to create mock home");

        Self {
            temp,
            project_dir: None,
            mock_home,
            mock_agent: None,
            rich_skill_name: None,
        }
    }

    /// Get the temp directory root path.
    pub fn temp_path(&self) -> &Path {
        self.temp.path()
    }

    /// Initialize project structure (.skillc/config.toml).
    /// Returns self for chaining.
    pub fn with_project(mut self) -> Self {
        let project_dir = self.temp.path().to_path_buf();
        let skillc_dir = project_dir.join(".skillc");
        fs::create_dir_all(&skillc_dir).expect("failed to create .skillc dir");
        fs::write(skillc_dir.join("config.toml"), "").expect("failed to write config");
        self.project_dir = Some(project_dir);
        self
    }

    /// Create mock agent directory for deployment testing.
    /// Returns self for chaining.
    pub fn with_mock_agent(mut self) -> Self {
        let mock_agent = self.temp.path().join("mock-agent");
        fs::create_dir_all(&mock_agent).expect("failed to create mock agent");
        self.mock_agent = Some(mock_agent);
        self
    }

    /// Get project directory. Panics if not initialized.
    pub fn project_dir(&self) -> &Path {
        self.project_dir
            .as_ref()
            .expect("project not initialized - call with_project() first")
    }

    /// Get mock home directory (always available).
    pub fn mock_home(&self) -> &Path {
        &self.mock_home
    }

    /// Get mock agent directory. Panics if not initialized.
    pub fn mock_agent(&self) -> &Path {
        self.mock_agent
            .as_ref()
            .expect("mock agent not initialized - call with_mock_agent() first")
    }

    /// Get mock agent path as string (for CLI args).
    pub fn mock_agent_str(&self) -> &str {
        self.mock_agent().to_str().expect("path should be UTF-8")
    }

    /// Create a skill in the project source store (.skillc/skills/<name>/).
    pub fn create_skill(&self, name: &str) -> PathBuf {
        let skills_dir = self.project_dir().join(".skillc").join("skills");
        create_test_skill(&skills_dir, name)
    }

    /// Create an external skill (outside project, for import testing).
    pub fn create_external_skill(&self, name: &str) -> PathBuf {
        let external_dir = self.temp.path().join("external");
        create_test_skill(&external_dir, name)
    }

    /// Create a skill with custom SKILL.md content.
    pub fn create_skill_with_content(&self, name: &str, content: &str) -> PathBuf {
        let skill_dir = self.project_dir().join(".skillc").join("skills").join(name);
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        fs::write(skill_dir.join("SKILL.md"), content).expect("failed to write SKILL.md");
        skill_dir
    }

    /// Create a global skill in mock_home's .skillc/skills/.
    pub fn create_global_skill(&self, name: &str) -> PathBuf {
        let global_skills = self.mock_home().join(".skillc").join("skills");
        fs::create_dir_all(&global_skills).expect("failed to create global skills dir");
        let skill_dir = global_skills.join(name);
        fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: Global test skill\n---\n# {}\n",
                name, name
            ),
        )
        .expect("failed to write SKILL.md");
        skill_dir
    }

    /// Ensure global skills directory exists in mock_home (empty).
    pub fn ensure_global_skills_dir(&self) {
        let global_skills = self.mock_home().join(".skillc").join("skills");
        fs::create_dir_all(&global_skills).expect("failed to create global skills dir");
    }

    /// Create a rich skill with multiple sections and docs for snapshot testing.
    /// This creates the same skill structure as the legacy init_workspace().
    pub fn with_rich_skill(mut self, name: &str) -> Self {
        // Ensure project exists
        if self.project_dir.is_none() {
            self = self.with_project();
        }

        let skill_dir = self.project_dir().join(".skillc").join("skills").join(name);
        fs::create_dir_all(skill_dir.join("docs")).expect("failed to create docs dir");

        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                r#"---
name: {}
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
                name
            ),
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

        self.rich_skill_name = Some(name.to_string());
        self
    }

    /// Get the name of the rich skill. Panics if not initialized.
    pub fn skill_name(&self) -> &str {
        self.rich_skill_name
            .as_ref()
            .expect("rich skill not initialized - call with_rich_skill() first")
    }

    /// Run skc command with automatic SKILLC_HOME isolation.
    /// Uses project_dir as working directory (or temp_path if no project).
    ///
    /// Per [[RFC-0009:C-ENV-OVERRIDE]], all commands use `SKILLC_HOME` for cross-platform isolation.
    pub fn run_skc(&self, args: &[&str]) -> TestResult {
        let cwd = self.project_dir.as_deref().unwrap_or(self.temp.path());
        self.run_skc_in(cwd, args)
    }

    /// Run skc command in a specific directory with automatic SKILLC_HOME isolation.
    ///
    /// Per [[RFC-0009:C-ENV-OVERRIDE]], all commands use `SKILLC_HOME` for cross-platform isolation.
    pub fn run_skc_in(&self, cwd: &Path, args: &[&str]) -> TestResult {
        let output = Command::new(assert_cmd::cargo::cargo_bin!("skc"))
            .args(args)
            .current_dir(cwd)
            .env("NO_COLOR", "1")
            .env("SKC_RUN_ID", "TEST-RUN-ID")
            .env("SKILLC_HOME", &self.mock_home)
            .output()
            .expect("failed to run skc");

        TestResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
        }
    }

    /// Run skc command with additional environment variables.
    ///
    /// Per [[RFC-0009:C-ENV-OVERRIDE]], SKILLC_HOME is always set for isolation.
    pub fn run_skc_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> TestResult {
        let cwd = self.project_dir.as_deref().unwrap_or(self.temp.path());
        self.run_skc_with_env_in(cwd, args, env)
    }

    /// Run skc command with additional environment variables in a specific directory.
    ///
    /// Per [[RFC-0009:C-ENV-OVERRIDE]], SKILLC_HOME is always set for isolation.
    pub fn run_skc_with_env_in(
        &self,
        cwd: &Path,
        args: &[&str],
        env: &[(&str, &str)],
    ) -> TestResult {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("skc"));
        cmd.args(args)
            .current_dir(cwd)
            .env("NO_COLOR", "1")
            .env("SKC_RUN_ID", "TEST-RUN-ID")
            .env("SKILLC_HOME", &self.mock_home);

        for (key, value) in env {
            cmd.env(key, value);
        }

        let output = cmd.output().expect("failed to run skc");

        TestResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            success: output.status.success(),
        }
    }

    /// Run skc and return normalized text output (for snapshots).
    /// Panics if command fails.
    pub fn run_skc_text(&self, args: &[&str]) -> String {
        let result = self.run_skc(args);
        if !result.success {
            panic!("skc {:?} failed: stderr={}", args, result.stderr);
        }
        normalize_text(&result.stdout, self.temp.path())
    }

    /// Run skc and return normalized JSON output (for snapshots).
    /// Panics if command fails.
    pub fn run_skc_json(&self, args: &[&str]) -> String {
        let result = self.run_skc(args);
        if !result.success {
            panic!("skc {:?} failed: stderr={}", args, result.stderr);
        }
        normalize_json(&result.stdout, self.temp.path())
    }

    /// Run skc and return normalized text output, allowing failure.
    pub fn run_skc_text_allow_fail(&self, args: &[&str]) -> String {
        let result = self.run_skc(args);
        normalize_text(&result.stdout, self.temp.path())
    }
}

impl Default for TestContext {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Standalone helpers (for specialized test cases)
// ============================================================================

/// Create a minimal skill directory with SKILL.md at an arbitrary path.
/// Used for creating skills outside the project structure (e.g., in runtime dirs).
pub fn create_minimal_skill(dir: &Path, name: &str) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).expect("failed to create skill dir");
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {}\ndescription: test\n---\n# {}\n", name, name),
    )
    .expect("failed to write SKILL.md");
}

/// Create a test skill with triggers (for build tests).
/// Used internally by TestContext.create_skill().
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

/// Get the path to the fallback logs database for a skill.
/// Used by sync tests.
pub fn fallback_db_path(project_dir: &Path, skill_name: &str) -> PathBuf {
    project_dir
        .join(".skillc")
        .join("logs")
        .join(skill_name)
        .join(".skillc-meta")
        .join("logs.db")
}

/// Get the path to the runtime logs database for a skill in a mock home.
/// Uses the SSOT runtime location (~/.skillc/runtime/<skill>/).
/// Used by sync tests.
pub fn runtime_db_path(mock_home: &Path, skill_name: &str) -> PathBuf {
    mock_home
        .join(".skillc")
        .join("runtime")
        .join(skill_name)
        .join(".skillc-meta")
        .join("logs.db")
}

/// Normalize a string by replacing tempdir paths and timestamps with placeholders.
fn normalize_string(s: &str, dir: &Path) -> String {
    // Normalize Windows path separators first
    let normalized = s.replace('\\', "/");
    let dir_str = dir.display().to_string().replace('\\', "/");

    // Also get canonicalized path (for Windows extended-length path matching)
    let canonical_dir_str = dir
        .canonicalize()
        .map(|p| p.display().to_string().replace('\\', "/"))
        .unwrap_or_else(|_| dir_str.clone());

    // Handle macOS /private prefix
    let mut normalized = normalized.replace(&format!("/private{}", dir_str), "<TEMPDIR>");
    // Handle Windows extended-length path prefix from canonicalize()
    // The canonical path on Windows already includes \\?\ which becomes //?/
    if canonical_dir_str.starts_with("//?/") {
        normalized = normalized.replace(&canonical_dir_str, "<TEMPDIR>");
    } else {
        normalized = normalized.replace(&format!("//?/{}", dir_str), "<TEMPDIR>");
    }
    normalized = normalized.replace(&dir_str, "<TEMPDIR>");

    let ts_pattern =
        Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})")
            .expect("valid timestamp regex");
    ts_pattern.replace_all(&normalized, "<TS>").to_string()
}

pub fn normalize_text(output: &str, dir: &Path) -> String {
    normalize_string(output, dir).trim_end().to_string()
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
            *s = normalize_string(s, dir);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_text_unix_path() {
        let dir = Path::new("/tmp/test123");
        let output = "Path: /tmp/test123/.skillc/skills/test-skill";
        let result = normalize_text(output, dir);
        assert_eq!(result, "Path: <TEMPDIR>/.skillc/skills/test-skill");
    }

    #[test]
    fn test_normalize_text_macos_private_prefix() {
        let dir = Path::new("/tmp/test123");
        let output = "Path: /private/tmp/test123/.skillc/skills/test-skill";
        let result = normalize_text(output, dir);
        assert_eq!(result, "Path: <TEMPDIR>/.skillc/skills/test-skill");
    }

    #[test]
    fn test_normalize_text_windows_backslashes() {
        // Simulate Windows output with backslashes
        let dir = Path::new("C:\\Users\\test\\AppData\\Local\\Temp\\.tmp123");
        let output =
            "Path: C:\\Users\\test\\AppData\\Local\\Temp\\.tmp123\\.skillc\\skills\\test-skill";
        let result = normalize_text(output, dir);
        assert_eq!(result, "Path: <TEMPDIR>/.skillc/skills/test-skill");
    }

    #[test]
    fn test_normalize_text_windows_extended_path() {
        // Simulate Windows canonicalized output with \\?\ prefix
        let dir = Path::new("C:\\Users\\test\\AppData\\Local\\Temp\\.tmp123");
        let output = "Path: \\\\?\\C:\\Users\\test\\AppData\\Local\\Temp\\.tmp123\\.skillc\\skills\\test-skill";
        let result = normalize_text(output, dir);
        assert_eq!(result, "Path: <TEMPDIR>/.skillc/skills/test-skill");
    }

    #[test]
    fn test_normalize_json_windows_extended_path() {
        let dir = Path::new("C:\\Users\\test\\AppData\\Local\\Temp\\.tmp123");
        let output = r#"{"skill_path": "\\\\?\\C:\\Users\\test\\AppData\\Local\\Temp\\.tmp123\\.skillc\\skills\\test-skill"}"#;
        let result = normalize_json(output, dir);
        assert!(
            result.contains("<TEMPDIR>/.skillc/skills/test-skill"),
            "Expected normalized path, got: {}",
            result
        );
    }
}
