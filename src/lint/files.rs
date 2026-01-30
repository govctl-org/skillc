//! File lint rules (SKL401) per [[RFC-0008:C-REGISTRY]]

use super::markdown::ExtractedLink;
use super::{Diagnostic, LintContext, LintResult, progress_bar};
use crate::error::Result;
use indicatif::ProgressIterator;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Files that are not considered orphaned per [[RFC-0008:C-REGISTRY]] SKL401
const EXCEPTION_FILES: &[&str] = &["LICENSE.md", "CHANGELOG.md", "CONTRIBUTING.md", "README.md"];

/// Lint file rules (SKL401).
///
/// Uses shared LintContext to avoid repeated file reads and parsing.
pub fn lint_files(skill_path: &Path, ctx: &LintContext, result: &mut LintResult) -> Result<()> {
    // Build reachability graph starting from SKILL.md
    let skill_md = skill_path.join("SKILL.md");
    let reachable = compute_reachable_files(&skill_md, skill_path, ctx);

    let pb = progress_bar("Checking orphans", ctx.md_files.len());
    let exception_set: HashSet<&str> = EXCEPTION_FILES.iter().copied().collect();

    for file_path in ctx.md_files.iter().progress_with(pb) {
        let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        // Skip SKILL.md and exception files
        if file_name == "SKILL.md" || exception_set.contains(file_name) {
            continue;
        }

        // Check if reachable
        if let Ok(canonical) = file_path.canonicalize()
            && !reachable.contains(&canonical)
        {
            let relative = file_path.strip_prefix(skill_path).unwrap_or(file_path);
            result.add(
                Diagnostic::warning(
                    "SKL401",
                    "no-orphans",
                    format!("orphaned file: '{}'", relative.display()),
                )
                .with_file(relative),
            );
        }
    }

    Ok(())
}

/// Compute the set of files reachable from SKILL.md via link traversal.
///
/// Uses cached content from LintContext to avoid repeated file reads.
fn compute_reachable_files(start: &Path, skill_path: &Path, ctx: &LintContext) -> HashSet<PathBuf> {
    let mut reachable: HashSet<PathBuf> = HashSet::new();
    let mut to_visit: Vec<PathBuf> = vec![start.to_path_buf()];

    while let Some(current) = to_visit.pop() {
        // Canonicalize for consistent comparison
        let canonical = match current.canonicalize() {
            Ok(p) => p,
            Err(_) => continue,
        };

        if reachable.contains(&canonical) {
            continue;
        }
        reachable.insert(canonical.clone());

        // Use cached links from context
        if let Some(cached) = ctx.get(&current) {
            for target in extract_link_targets_from_cached(&cached.links, &current, skill_path) {
                if !reachable.contains(&target) {
                    to_visit.push(target);
                }
            }
        }
    }

    reachable
}

/// Extract link targets from pre-parsed links that point to .md files.
///
/// Uses cached links to avoid re-parsing.
fn extract_link_targets_from_cached(
    links: &[ExtractedLink],
    from_file: &Path,
    skill_path: &Path,
) -> Vec<PathBuf> {
    let mut targets = Vec::new();

    for link in links {
        let link_target = &link.dest;

        // Skip external/absolute
        if link_target.starts_with("http") || link_target.starts_with('/') || link_target.is_empty()
        {
            continue;
        }

        // Get path part (before #)
        let path_part = link_target.split('#').next().unwrap_or("");
        if path_part.is_empty() {
            continue;
        }

        // Resolve relative to from_file's directory
        let target = from_file.parent().unwrap_or(skill_path).join(path_part);

        // Only consider .md files that exist
        if target.exists()
            && target.extension().is_some_and(|e| e == "md")
            && let Ok(canonical) = target.canonicalize()
        {
            targets.push(canonical);
        }
    }

    targets
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_result() -> LintResult {
        LintResult::new("test".to_string(), PathBuf::new())
    }

    fn make_context(skill_path: &Path) -> LintContext {
        LintContext::new(skill_path).expect("create lint context")
    }

    #[test]
    fn test_orphaned_file_detected() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        // Create SKILL.md without links
        fs::write(skill_path.join("SKILL.md"), "# Test\n\nNo links here.\n")
            .expect("write test file");

        // Create orphaned file
        fs::write(skill_path.join("orphan.md"), "# Orphan\n").expect("write test file");

        let ctx = make_context(skill_path);
        let mut result = make_result();
        lint_files(skill_path, &ctx, &mut result).expect("lint files");

        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| { d.rule_id == "SKL401" && d.message.contains("orphan.md") })
        );
    }

    #[test]
    fn test_linked_file_not_orphaned() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        // Create SKILL.md with link
        fs::write(skill_path.join("SKILL.md"), "# Test\n\n[ref](ref.md)\n")
            .expect("write test file");

        // Create linked file
        fs::write(skill_path.join("ref.md"), "# Reference\n").expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_files(skill_path, &ctx, &mut result).expect("lint files");

        // Should not flag ref.md as orphaned
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|d| { d.rule_id == "SKL401" && d.message.contains("ref.md") })
        );
    }

    #[test]
    fn test_transitive_reachability() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        // SKILL.md -> a.md -> b.md
        fs::write(skill_path.join("SKILL.md"), "# Test\n\n[a](a.md)\n").expect("write test file");
        fs::write(skill_path.join("a.md"), "# A\n\n[b](b.md)\n").expect("write test file");
        fs::write(skill_path.join("b.md"), "# B\n").expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_files(skill_path, &ctx, &mut result).expect("lint files");

        // Neither a.md nor b.md should be flagged
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_exception_files_not_orphaned() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(skill_path.join("SKILL.md"), "# Test\n").expect("write test file");
        fs::write(skill_path.join("README.md"), "# README\n").expect("write test file");
        fs::write(skill_path.join("LICENSE.md"), "# LICENSE\n").expect("write test file");
        fs::write(skill_path.join("CHANGELOG.md"), "# CHANGELOG\n").expect("write test file");
        fs::write(skill_path.join("CONTRIBUTING.md"), "# CONTRIBUTING\n").expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_files(skill_path, &ctx, &mut result).expect("lint files");

        // None of the exception files should be flagged
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_hidden_directory_excluded() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(skill_path.join("SKILL.md"), "# Test\n").expect("write test file");

        // Create hidden directory with file
        let hidden_dir = skill_path.join(".hidden");
        fs::create_dir_all(&hidden_dir).expect("create test dir");
        fs::write(hidden_dir.join("hidden.md"), "# Hidden\n").expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_files(skill_path, &ctx, &mut result).expect("lint files");

        // Hidden file should not be flagged (not even scanned)
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_nested_orphan() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        fs::write(skill_path.join("SKILL.md"), "# Test\n").expect("write test file");

        // Create nested orphan
        let sub_dir = skill_path.join("subdir");
        fs::create_dir_all(&sub_dir).expect("create test dir");
        fs::write(sub_dir.join("orphan.md"), "# Orphan\n").expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_files(skill_path, &ctx, &mut result).expect("lint files");

        assert!(
            result
                .diagnostics
                .iter()
                .any(|d| { d.rule_id == "SKL401" && d.message.contains("orphan.md") })
        );
    }

    #[test]
    fn test_circular_links() {
        let dir = TempDir::new().expect("create temp dir");
        let skill_path = dir.path();

        // Create circular links: SKILL.md -> a.md -> b.md -> a.md
        fs::write(skill_path.join("SKILL.md"), "# Test\n\n[a](a.md)\n").expect("write test file");
        fs::write(skill_path.join("a.md"), "# A\n\n[b](b.md)\n").expect("write test file");
        fs::write(skill_path.join("b.md"), "# B\n\n[a](a.md)\n").expect("write test file");

        let mut result = make_result();
        let ctx = make_context(skill_path);
        lint_files(skill_path, &ctx, &mut result).expect("lint files");

        // Should handle circular links without infinite loop
        assert!(result.diagnostics.is_empty());
    }
}
