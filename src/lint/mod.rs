//! Skill linting per [[RFC-0008]]
//!
//! Implements SKLxxx lint rules for validating skill authoring quality.
//!
//! # Rule Categories
//!
//! - SKL0xx: Meta rules (skip-compiled)
//! - SKL1xx: Frontmatter rules
//! - SKL2xx: Structure rules
//! - SKL3xx: Link rules
//! - SKL4xx: File rules

mod files;
mod frontmatter;
mod links;
mod structure;

use crate::error::Result;
use crate::markdown::ExtractedLink;
use crate::verbose;
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressStyle};
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Create a styled progress bar
///
/// Returns a hidden progress bar when:
/// - stderr is not a terminal (piped output)
/// - `CI` environment variable is set (running in CI)
fn progress_bar(msg: &str, len: usize) -> ProgressBar {
    let in_ci = std::env::var_os("CI").is_some();
    let pb = if std::io::stderr().is_terminal() && !in_ci {
        ProgressBar::new(len as u64)
    } else {
        ProgressBar::hidden()
    };
    if let Ok(style) = ProgressStyle::default_bar()
        .template("{spinner:.green} {msg} [{bar:30.cyan/blue}] {pos}/{len}")
    {
        pb.set_style(style.progress_chars("=>-"));
    }
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(100));
    pb
}

pub use frontmatter::parse_frontmatter;

/// Cached file data to avoid repeated parsing
struct CachedFile {
    /// Raw file content
    content: String,
    /// Extracted links (parsed once using pulldown-cmark AST)
    links: Vec<ExtractedLink>,
}

// CachedFile is Send + Sync since it only contains String and Vec<ExtractedLink>
unsafe impl Send for CachedFile {}
unsafe impl Sync for CachedFile {}

/// Shared context for linting to avoid repeated file reads and parsing
struct LintContext {
    /// All markdown files in the skill directory
    md_files: Vec<PathBuf>,
    /// Cached file contents and parsed links
    cache: HashMap<PathBuf, CachedFile>,
}

impl LintContext {
    /// Create a new lint context, collecting all markdown files and pre-parsing them.
    ///
    /// Uses rayon for parallel file parsing with progress indicator.
    /// Stores files by canonical path for consistent lookups.
    fn new(skill_path: &Path) -> Result<Self> {
        let md_files = collect_md_files(skill_path)?;
        let pb = progress_bar("Parsing", md_files.len());

        let parsed: Vec<_> = md_files
            .par_iter()
            .progress_with(pb)
            .filter_map(|file_path| {
                fs::read_to_string(file_path).ok().map(|content| {
                    let links = crate::markdown::extract_links(&content);
                    let key = file_path
                        .canonicalize()
                        .unwrap_or_else(|_| file_path.clone());
                    (key, CachedFile { content, links })
                })
            })
            .collect();

        let cache: HashMap<PathBuf, CachedFile> = parsed.into_iter().collect();

        Ok(Self { md_files, cache })
    }

    /// Get cached file data by path.
    ///
    /// Canonicalizes the path before lookup for consistency.
    fn get(&self, path: &Path) -> Option<&CachedFile> {
        let key = path.canonicalize().ok()?;
        self.cache.get(&key)
    }
}

/// Collect all .md files in skill directory, excluding hidden directories
fn collect_md_files(skill_path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(skill_path)
        .into_iter()
        .filter_entry(|e| {
            // Allow root directory, but exclude hidden directories within
            e.depth() == 0 || !e.file_name().to_string_lossy().starts_with('.')
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        files.push(entry.path().to_path_buf());
    }

    Ok(files)
}

/// Options for the lint command
#[derive(Debug, Clone, Default)]
pub struct LintOptions {
    /// Force linting even on compiled skills
    pub force: bool,
}

/// Lint diagnostic severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Severity {
    Error,
    Warning,
}

/// A lint diagnostic per [[RFC-0005:C-CODES]]
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// Rule ID (e.g., "SKL102")
    pub rule_id: String,
    /// Rule name (e.g., "name-format")
    pub rule_name: String,
    /// Severity (error or warning)
    pub severity: Severity,
    /// Human-readable message
    pub message: String,
    /// Optional file path (relative to skill root)
    pub file: Option<PathBuf>,
    /// Optional line number (1-indexed)
    pub line: Option<usize>,
}

impl Diagnostic {
    /// Create a new error diagnostic
    pub fn error(rule_id: &str, rule_name: &str, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            rule_name: rule_name.to_string(),
            severity: Severity::Error,
            message: message.into(),
            file: None,
            line: None,
        }
    }

    /// Create a new warning diagnostic
    pub fn warning(rule_id: &str, rule_name: &str, message: impl Into<String>) -> Self {
        Self {
            rule_id: rule_id.to_string(),
            rule_name: rule_name.to_string(),
            severity: Severity::Warning,
            message: message.into(),
            file: None,
            line: None,
        }
    }

    /// Set file path
    pub fn with_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.file = Some(path.into());
        self
    }

    /// Set line number
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format like compiler: <file>:<line>: <severity>[<code>]: <rule-id> <rule-name>: <message>
        // Per [[RFC-0005:C-CODES]] with location prefix
        let (severity_str, code) = match self.severity {
            Severity::Error => ("error", "E300"),
            Severity::Warning => ("warning", "W300"),
        };

        // Build location prefix
        match (&self.file, self.line) {
            (Some(file), Some(line)) => {
                write!(f, "{}:{}: ", file.display(), line)?;
            }
            (Some(file), None) => {
                write!(f, "{}: ", file.display())?;
            }
            (None, Some(line)) => {
                write!(f, ":{}: ", line)?;
            }
            (None, None) => {}
        }

        write!(
            f,
            "{}[{}]: {} {}: {}",
            severity_str, code, self.rule_id, self.rule_name, self.message
        )
    }
}

/// Result of linting a skill
#[derive(Debug, Serialize)]
pub struct LintResult {
    /// Skill name
    pub skill: String,
    /// Skill path
    pub path: PathBuf,
    /// Diagnostics found
    pub diagnostics: Vec<Diagnostic>,
    /// Number of errors
    pub error_count: usize,
    /// Number of warnings
    pub warning_count: usize,
}

impl LintResult {
    /// Create a new empty lint result
    pub fn new(skill: String, path: PathBuf) -> Self {
        Self {
            skill,
            path,
            diagnostics: Vec::new(),
            error_count: 0,
            warning_count: 0,
        }
    }

    /// Add a diagnostic
    pub fn add(&mut self, diag: Diagnostic) {
        match diag.severity {
            Severity::Error => self.error_count += 1,
            Severity::Warning => self.warning_count += 1,
        }
        self.diagnostics.push(diag);
    }

    /// Check if there are any errors
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// Check if there are any diagnostics
    pub fn has_diagnostics(&self) -> bool {
        !self.diagnostics.is_empty()
    }
}

/// Lint a skill directory per [[RFC-0008]].
///
/// Returns diagnostics and exits with error if any error-severity rules are violated.
pub fn lint(skill_path: &Path, options: LintOptions) -> Result<LintResult> {
    verbose!("lint: skill_path={}", skill_path.display());

    // Validate skill path (E001/E010 per [[RFC-0005:C-CODES]])
    crate::util::validate_skill_path(skill_path)?;
    let skill_md_path = skill_path.join("SKILL.md");

    // Extract skill name from directory
    let skill_name = skill_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let mut result = LintResult::new(skill_name.clone(), skill_path.to_path_buf());

    // Check for compiled skill per [[RFC-0008:C-REGISTRY]] SKL001
    let manifest_path = skill_path.join(".skillc-meta").join("manifest.json");
    if manifest_path.exists() {
        if !options.force {
            println!("info: skipping compiled skill '{}'", skill_name);
            return Ok(result);
        }
        // Emit warning when forcing on compiled skill
        result.add(Diagnostic::warning(
            "SKL001",
            "skip-compiled",
            "linting compiled skill; results may not be meaningful",
        ));
    }

    // Read SKILL.md content
    let skill_md_content = fs::read_to_string(&skill_md_path)?;

    // Run frontmatter rules (SKL100-SKL109)
    frontmatter::lint_frontmatter(
        &skill_md_content,
        &skill_md_path,
        skill_path,
        &skill_name,
        &mut result,
    )?;

    // Run structure rules (SKL201-SKL203) for SKILL.md
    structure::lint_structure(
        &skill_md_content,
        &skill_md_path,
        skill_path,
        &skill_name,
        &mut result,
    );

    // Build shared context (collects and parses all files in parallel)
    let ctx = LintContext::new(skill_path)?;

    // Run heading hierarchy rules (SKL204-SKL205) for all markdown files
    for file_path in &ctx.md_files {
        if let Some(cached) = ctx.get(file_path) {
            structure::lint_heading_hierarchy(&cached.content, file_path, skill_path, &mut result);
        }
    }

    // Run link rules (SKL301-SKL303) with parallel checking
    links::lint_links(skill_path, &ctx, &mut result)?;

    // Run file rules (SKL401)
    files::lint_files(skill_path, &ctx, &mut result)?;

    verbose!(
        "lint: {} errors, {} warnings",
        result.error_count,
        result.warning_count
    );

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Create a test skill with given SKILL.md content
    fn create_test_skill(content: &str) -> (TempDir, PathBuf) {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path().join("test-skill");
        fs::create_dir_all(&skill_path).expect("create skill dir");
        fs::write(skill_path.join("SKILL.md"), content).expect("write test file");
        (dir, skill_path)
    }

    #[test]
    fn test_lint_missing_skill_md() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path().join("no-skill");
        fs::create_dir_all(&skill_path).expect("create skill dir");

        let result = lint(&skill_path, LintOptions::default());
        assert!(result.is_err());
    }

    #[test]
    fn test_lint_valid_skill() {
        let (_dir, skill_path) = create_test_skill(
            r#"---
name: test-skill
description: "A test skill. Use when testing."
---

# Test Skill

This is a test.
"#,
        );

        let result = lint(&skill_path, LintOptions::default()).expect("lint skill");
        assert_eq!(result.error_count, 0);
        // May have warnings (like SKL104 name-match-dir if dir name differs)
    }

    #[test]
    fn test_lint_missing_frontmatter() {
        let (_dir, skill_path) = create_test_skill("# No frontmatter\n\nJust content.");

        let result = lint(&skill_path, LintOptions::default()).expect("lint skill");
        assert!(result.error_count > 0);
        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL100"));
    }

    #[test]
    fn test_lint_missing_name() {
        let (_dir, skill_path) = create_test_skill(
            r#"---
description: "A skill without name"
---

# Test
"#,
        );

        let result = lint(&skill_path, LintOptions::default()).expect("lint skill");
        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL101"));
    }

    #[test]
    fn test_lint_invalid_name_format() {
        let (_dir, skill_path) = create_test_skill(
            r#"---
name: My-Skill
description: "Uppercase name"
---

# Test
"#,
        );

        let result = lint(&skill_path, LintOptions::default()).expect("lint skill");
        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL102"));
    }

    #[test]
    fn test_lint_compiled_skill_skipped() {
        let (_dir, skill_path) = create_test_skill(
            r#"---
name: compiled
description: "A compiled skill"
---

# Compiled
"#,
        );

        // Create .skillc-meta/manifest.json to mark as compiled
        let meta_dir = skill_path.join(".skillc-meta");
        fs::create_dir_all(&meta_dir).expect("create meta dir");
        fs::write(meta_dir.join("manifest.json"), "{}").expect("write test file");

        let result = lint(&skill_path, LintOptions::default()).expect("lint skill");
        // Should have no diagnostics because it was skipped
        assert_eq!(result.diagnostics.len(), 0);
    }

    #[test]
    fn test_lint_compiled_skill_forced() {
        let (_dir, skill_path) = create_test_skill(
            r#"---
name: compiled
description: "A compiled skill"
---

# Compiled
"#,
        );

        // Create .skillc-meta/manifest.json to mark as compiled
        let meta_dir = skill_path.join(".skillc-meta");
        fs::create_dir_all(&meta_dir).expect("create meta dir");
        fs::write(meta_dir.join("manifest.json"), "{}").expect("write test file");

        let result = lint(&skill_path, LintOptions { force: true }).expect("lint skill with force");
        // Should have SKL001 warning
        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL001"));
    }

    #[test]
    fn test_diagnostic_display() {
        // Without file/line
        let diag = Diagnostic::error("SKL102", "name-format", "invalid name");
        assert_eq!(
            diag.to_string(),
            "error[E300]: SKL102 name-format: invalid name"
        );

        let diag = Diagnostic::warning("SKL104", "name-match-dir", "mismatch");
        assert_eq!(
            diag.to_string(),
            "warning[W300]: SKL104 name-match-dir: mismatch"
        );

        // With file only
        let diag =
            Diagnostic::error("SKL301", "link-file-exists", "not found").with_file("docs/guide.md");
        assert_eq!(
            diag.to_string(),
            "docs/guide.md: error[E300]: SKL301 link-file-exists: not found"
        );

        // With file and line
        let diag = Diagnostic::error("SKL301", "link-file-exists", "not found")
            .with_file("docs/guide.md")
            .with_line(42);
        assert_eq!(
            diag.to_string(),
            "docs/guide.md:42: error[E300]: SKL301 link-file-exists: not found"
        );
    }
}
