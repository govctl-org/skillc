//! Link lint rules (SKL301-SKL303) per [[RFC-0008:C-REGISTRY]]

use super::{Diagnostic, LintContext, LintResult, progress_bar};
use crate::error::Result;
use crate::markdown::extract_headings;
use indicatif::ParallelProgressIterator;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

/// Lint link rules SKL301-SKL303.
///
/// Uses pulldown-cmark AST to properly skip links inside code blocks/inline code.
/// Uses shared LintContext to avoid repeated file reads and parsing.
/// Processes files in parallel using rayon with progress indicator.
pub fn lint_links(skill_path: &Path, ctx: &LintContext, result: &mut LintResult) -> Result<()> {
    let pb = progress_bar("Checking links", ctx.md_files.len());

    let diagnostics: Vec<Diagnostic> = ctx
        .md_files
        .par_iter()
        .progress_with(pb)
        .flat_map(|file_path| lint_file_links(skill_path, file_path, ctx))
        .collect();

    for diag in diagnostics {
        result.add(diag);
    }

    Ok(())
}

/// Lint links in a single file (called in parallel)
fn lint_file_links(skill_path: &Path, file_path: &Path, ctx: &LintContext) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let cached = match ctx.get(file_path) {
        Some(c) => c,
        None => return diagnostics,
    };
    let relative_path = file_path.strip_prefix(skill_path).unwrap_or(file_path);

    // Use pre-parsed links from context
    for link in &cached.links {
        let link_target = &link.dest;
        let line_num = link.line;

        // Skip external URLs and absolute paths
        if is_external_or_absolute(link_target) {
            continue;
        }

        // Split into path and anchor
        let (path_part, anchor_part) = split_link_target(link_target);

        // SKL303: link-no-escape
        if path_part.contains("..") && link_escapes_root(file_path, path_part, skill_path) {
            diagnostics.push(
                Diagnostic::error(
                    "SKL303",
                    "link-no-escape",
                    format!("link '{}' escapes skill root", link_target),
                )
                .with_file(relative_path)
                .with_line(line_num),
            );
            continue; // Don't check existence if it escapes
        }

        // SKL301: link-file-exists
        if !path_part.is_empty() {
            let target_path = file_path.parent().unwrap_or(skill_path).join(path_part);
            if !target_path.exists() {
                diagnostics.push(
                    Diagnostic::error(
                        "SKL301",
                        "link-file-exists",
                        format!("link target not found: '{}'", path_part),
                    )
                    .with_file(relative_path)
                    .with_line(line_num),
                );
                continue; // Don't check anchor if file doesn't exist
            }
        }

        // SKL302: link-anchor-exists
        if let Some(anchor) = anchor_part
            && !anchor.is_empty()
        {
            let target_file = if path_part.is_empty() {
                file_path.to_path_buf()
            } else {
                file_path.parent().unwrap_or(skill_path).join(path_part)
            };

            // Try to get content from cache first, fall back to reading
            let target_content = if let Some(cached) = ctx.get(&target_file) {
                Some(cached.content.as_str())
            } else if target_file.exists() {
                None // Will read below if needed
            } else {
                continue; // File doesn't exist, skip anchor check
            };

            let content_for_check: std::borrow::Cow<str> = match target_content {
                Some(c) => std::borrow::Cow::Borrowed(c),
                None => match fs::read_to_string(&target_file) {
                    Ok(c) => std::borrow::Cow::Owned(c),
                    Err(_) => continue,
                },
            };

            let headings = extract_heading_anchors(&content_for_check);
            if !headings.contains(&anchor.to_lowercase()) {
                let target_display = if path_part.is_empty() {
                    relative_path.to_string_lossy().to_string()
                } else {
                    path_part.to_string()
                };
                diagnostics.push(
                    Diagnostic::warning(
                        "SKL302",
                        "link-anchor-exists",
                        format!("anchor '{}' not found in '{}'", anchor, target_display),
                    )
                    .with_file(relative_path)
                    .with_line(line_num),
                );
            }
        }
    }

    diagnostics
}

/// Check if a link target is external (http/https) or absolute
fn is_external_or_absolute(target: &str) -> bool {
    target.starts_with("http://") || target.starts_with("https://") || target.starts_with('/')
}

/// Split link target into path and anchor parts
fn split_link_target(target: &str) -> (&str, Option<&str>) {
    if let Some(idx) = target.find('#') {
        (&target[..idx], Some(&target[idx + 1..]))
    } else {
        (target, None)
    }
}

/// Check if a link escapes the skill root directory
fn link_escapes_root(from_file: &Path, link_path: &str, skill_path: &Path) -> bool {
    let link_full = from_file.parent().unwrap_or(skill_path).join(link_path);
    if let Ok(canonical) = link_full.canonicalize()
        && let Ok(skill_canonical) = skill_path.canonicalize()
    {
        return !canonical.starts_with(&skill_canonical);
    }
    // If we can't canonicalize, be conservative and allow
    false
}

/// Extract heading anchors from markdown content using GitHub-style slugging.
///
/// Per [[RFC-0008:C-REGISTRY]] SKL302:
/// 1. Convert to lowercase
/// 2. Remove characters except a-z, 0-9, spaces, hyphens
/// 3. Replace spaces with hyphens
/// 4. Collapse consecutive hyphens
///
/// Uses pulldown-cmark AST to properly extract heading text.
pub fn extract_heading_anchors(content: &str) -> HashSet<String> {
    let mut anchors = HashSet::new();
    let mut anchor_counts: HashMap<String, usize> = HashMap::new();

    for heading in extract_headings(content) {
        let heading_text = heading.text;
        let slug = github_slug(&heading_text);
        let count = anchor_counts.entry(slug.clone()).or_insert(0);
        if *count == 0 {
            anchors.insert(slug.clone());
        } else {
            anchors.insert(format!("{}-{}", slug, count));
        }
        *count += 1;
    }

    anchors
}

/// Convert heading text to GitHub-style anchor slug.
///
/// Per [[RFC-0008:C-REGISTRY]] SKL302:
/// - Convert to lowercase
/// - Remove characters except a-z, 0-9, spaces, hyphens (non-ASCII removed)
/// - Replace spaces with hyphens
/// - Collapse consecutive hyphens
pub fn github_slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_result() -> LintResult {
        LintResult::new("test".to_string(), std::path::PathBuf::new())
    }

    fn make_context(skill_path: &Path) -> LintContext {
        LintContext::new(skill_path).expect("create lint context")
    }

    #[test]
    fn test_github_slug() {
        assert_eq!(github_slug("Quick Start"), "quick-start");
        assert_eq!(github_slug("Quick Start!"), "quick-start");
        assert_eq!(github_slug("API Reference"), "api-reference");
        assert_eq!(github_slug("my-skill"), "my-skill");
        assert_eq!(github_slug("Section 1.2"), "section-12");
        assert_eq!(github_slug("  Multiple   Spaces  "), "multiple-spaces");
    }

    #[test]
    fn test_github_slug_non_ascii() {
        // Non-ASCII characters should be removed
        assert_eq!(github_slug("Résumé"), "rsum");
        assert_eq!(github_slug("日本語"), "");
        assert_eq!(github_slug("Test 日本語 End"), "test-end");
    }

    #[test]
    fn test_extract_heading_anchors() {
        let content = r#"# First
## Second
### Third
## Second
"#;
        let anchors = extract_heading_anchors(content);
        assert!(anchors.contains("first"));
        assert!(anchors.contains("second"));
        assert!(anchors.contains("third"));
        assert!(anchors.contains("second-1")); // duplicate
    }

    #[test]
    fn test_is_external_or_absolute() {
        assert!(is_external_or_absolute("http://example.com"));
        assert!(is_external_or_absolute("https://example.com"));
        assert!(is_external_or_absolute("/absolute/path"));

        assert!(!is_external_or_absolute("relative/path.md"));
        assert!(!is_external_or_absolute("./local.md"));
        assert!(!is_external_or_absolute("#anchor"));
    }

    #[test]
    fn test_split_link_target() {
        assert_eq!(split_link_target("file.md"), ("file.md", None));
        assert_eq!(
            split_link_target("file.md#anchor"),
            ("file.md", Some("anchor"))
        );
        assert_eq!(split_link_target("#anchor"), ("", Some("anchor")));
    }

    #[test]
    fn test_lint_broken_link() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(
            skill_path.join("SKILL.md"),
            "# Test\n\n[broken](nonexistent.md)\n",
        )
        .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL301"));
    }

    #[test]
    fn test_lint_valid_link() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(skill_path.join("SKILL.md"), "# Test\n\n[ref](ref.md)\n")
            .expect("write test file");
        fs::write(skill_path.join("ref.md"), "# Reference\n").expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        // Should have no SKL301 errors
        assert!(!result.diagnostics.iter().any(|d| d.rule_id == "SKL301"));
    }

    #[test]
    fn test_lint_broken_anchor() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(
            skill_path.join("SKILL.md"),
            "# Test\n\n[link](#nonexistent)\n",
        )
        .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL302"));
    }

    #[test]
    fn test_lint_valid_anchor() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(
            skill_path.join("SKILL.md"),
            "# Test\n\n## Section\n\n[link](#section)\n",
        )
        .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        // Should have no SKL302 warnings
        assert!(!result.diagnostics.iter().any(|d| d.rule_id == "SKL302"));
    }

    #[test]
    fn test_lint_external_links_skipped() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(
            skill_path.join("SKILL.md"),
            "# Test\n\n[external](https://example.com)\n",
        )
        .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        // Should have no errors (external links are skipped)
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_lint_hidden_dirs_excluded() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(skill_path.join("SKILL.md"), "# Test\n").expect("write test file");

        // Create hidden directory with markdown file
        let hidden_dir = skill_path.join(".hidden");
        fs::create_dir_all(&hidden_dir).expect("create test dir");
        fs::write(hidden_dir.join("secret.md"), "# Secret\n[broken](x.md)\n")
            .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        // Should not lint files in hidden directories
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_lint_links_in_code_block_skipped() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        // Link inside fenced code block should not be flagged
        fs::write(
            skill_path.join("SKILL.md"),
            r#"# Test

```markdown
[fake link](nonexistent.md)
```
"#,
        )
        .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        // Should have no SKL301 errors (link is inside code block)
        assert!(!result.diagnostics.iter().any(|d| d.rule_id == "SKL301"));
    }

    #[test]
    fn test_lint_links_in_inline_code_skipped() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        // Link syntax inside backticks should not be flagged
        fs::write(
            skill_path.join("SKILL.md"),
            "# Test\n\nUse `[text](url)` for links.\n",
        )
        .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        // Should have no SKL301 errors (link is inside inline code)
        assert!(!result.diagnostics.iter().any(|d| d.rule_id == "SKL301"));
    }

    #[test]
    fn test_lint_links_in_table_with_code_skipped() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        // This was the false positive case: link syntax inside backticks in a table
        fs::write(
            skill_path.join("SKILL.md"),
            r#"# Test

| Format | Markdown |
|--------|----------|
| Link | `[text](url)` |
"#,
        )
        .expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_links(skill_path, &ctx, &mut result).expect("lint links");

        // Should have no SKL301 errors (link is inside inline code in table)
        assert!(!result.diagnostics.iter().any(|d| d.rule_id == "SKL301"));
    }
}
