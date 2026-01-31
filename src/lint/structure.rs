//! Structure lint rules (SKL201-SKL205) per [[RFC-0008:C-REGISTRY]]

use super::{Diagnostic, LintResult};
use crate::markdown::extract_headings;
use lazy_regex::{Lazy, Regex, lazy_regex};
use std::path::Path;

/// Regex for markdown headings
static HEADING_RE: Lazy<Regex> = lazy_regex!(r"^(#{1,6})\s+(.+)$");

/// Maximum recommended SKILL.md line count per [[RFC-0008:C-REGISTRY]] SKL201
const MAX_SKILL_LINES: usize = 500;

/// Lint structure rules SKL201-SKL203
pub fn lint_structure(
    content: &str,
    file_path: &Path,
    skill_path: &Path,
    skill_name: &str,
    result: &mut LintResult,
) {
    let relative_path = file_path.strip_prefix(skill_path).unwrap_or(file_path);
    let lines: Vec<&str> = content.lines().collect();

    // SKL201: skill-size
    if lines.len() > MAX_SKILL_LINES {
        result.add(
            Diagnostic::warning(
                "SKL201",
                "skill-size",
                format!(
                    "SKILL.md exceeds {} lines ({} lines)",
                    MAX_SKILL_LINES,
                    lines.len()
                ),
            )
            .with_file(relative_path),
        );
    }

    // Find H1 headings
    let h1_headings = extract_h1_headings(&lines);

    // SKL202: heading-h1
    if h1_headings.is_empty() {
        result.add(
            Diagnostic::warning("SKL202", "heading-h1", "missing H1 heading in SKILL.md")
                .with_file(relative_path),
        );
    }

    // SKL203: heading-match-name
    if let Some((line_num, heading)) = h1_headings.first()
        && !heading_matches_name(heading, skill_name)
    {
        result.add(
            Diagnostic::warning(
                "SKL203",
                "heading-match-name",
                format!(
                    "H1 heading '{}' does not match skill name '{}'",
                    heading, skill_name
                ),
            )
            .with_file(relative_path)
            .with_line(*line_num),
        );
    }
}

/// Extract H1 headings with line numbers
fn extract_h1_headings(lines: &[&str]) -> Vec<(usize, String)> {
    let mut headings = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(caps) = HEADING_RE.captures(line) {
            let level = caps.get(1).map(|m| m.as_str().len()).unwrap_or(0);
            if level == 1 {
                let text = caps
                    .get(2)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                headings.push((i + 1, text)); // 1-indexed line number
            }
        }
    }
    headings
}

/// Check if heading matches skill name per [[RFC-0008:C-REGISTRY]] SKL203
///
/// Matching is case-insensitive. The heading may contain additional text
/// (e.g., "My Skill Guide" matches "my-skill").
fn heading_matches_name(heading: &str, skill_name: &str) -> bool {
    // Normalize: lowercase, replace hyphens with spaces
    let heading_normalized = heading.to_lowercase().replace('-', " ");
    let name_normalized = skill_name.to_lowercase().replace('-', " ");

    heading_normalized.contains(&name_normalized)
}

/// Lint heading hierarchy rules SKL204-SKL205 for a single file.
///
/// Per [[RFC-0008:C-REGISTRY]]:
/// - SKL204: First heading should be H1
/// - SKL205: Headings should not skip levels when going deeper
pub fn lint_heading_hierarchy(
    content: &str,
    file_path: &Path,
    skill_path: &Path,
    result: &mut LintResult,
) {
    let relative_path = file_path.strip_prefix(skill_path).unwrap_or(file_path);
    let headings = extract_headings(content);

    if headings.is_empty() {
        return;
    }

    // SKL204: heading-first-h1
    let first = &headings[0];
    if first.level != 1 {
        result.add(
            Diagnostic::warning(
                "SKL204",
                "heading-first-h1",
                format!("first heading is H{}, expected H1", first.level),
            )
            .with_file(relative_path)
            .with_line(first.line),
        );
    }

    // SKL205: heading-hierarchy
    let mut prev_level = 0;
    for heading in &headings {
        // Check for skipped levels when going deeper
        if heading.level > prev_level + 1 && prev_level > 0 {
            result.add(
                Diagnostic::warning(
                    "SKL205",
                    "heading-hierarchy",
                    format!(
                        "heading skips from H{} to H{} (expected H{})",
                        prev_level,
                        heading.level,
                        prev_level + 1
                    ),
                )
                .with_file(relative_path)
                .with_line(heading.line),
            );
        }
        prev_level = heading.level;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_result() -> LintResult {
        LintResult::new("test".to_string(), std::path::PathBuf::new())
    }

    #[test]
    fn test_extract_h1_headings() {
        let lines = vec!["# First", "## Second", "# Third", "text"];
        let headings = extract_h1_headings(&lines);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0], (1, "First".to_string()));
        assert_eq!(headings[1], (3, "Third".to_string()));
    }

    #[test]
    fn test_heading_matches_name() {
        assert!(heading_matches_name("my-skill", "my-skill"));
        assert!(heading_matches_name("My Skill", "my-skill"));
        assert!(heading_matches_name("MY-SKILL", "my-skill"));
        assert!(heading_matches_name("My Skill Guide", "my-skill"));
        assert!(heading_matches_name(
            "The my-skill Documentation",
            "my-skill"
        ));

        assert!(!heading_matches_name("other", "my-skill"));
        assert!(!heading_matches_name("myskill", "my-skill")); // no space/hyphen
    }

    #[test]
    fn test_lint_skill_too_large() {
        let content = "# Test\n".to_string() + &"line\n".repeat(550);
        let mut result = make_result();
        lint_structure(
            &content,
            Path::new("SKILL.md"),
            Path::new("."),
            "test",
            &mut result,
        );

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL201"));
    }

    #[test]
    fn test_lint_no_h1() {
        let content = "## Only H2\n\nContent here";
        let mut result = make_result();
        lint_structure(
            content,
            Path::new("SKILL.md"),
            Path::new("."),
            "test",
            &mut result,
        );

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL202"));
    }

    #[test]
    fn test_lint_h1_mismatch() {
        let content = "# Other Name\n\nContent";
        let mut result = make_result();
        lint_structure(
            content,
            Path::new("SKILL.md"),
            Path::new("."),
            "my-skill",
            &mut result,
        );

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL203"));
    }

    #[test]
    fn test_lint_h1_matches() {
        let content = "# My Skill\n\nContent";
        let mut result = make_result();
        lint_structure(
            content,
            Path::new("SKILL.md"),
            Path::new("."),
            "my-skill",
            &mut result,
        );

        // Should not have SKL203
        assert!(!result.diagnostics.iter().any(|d| d.rule_id == "SKL203"));
    }

    #[test]
    fn test_lint_valid_structure() {
        let content = "# Test\n\n".to_string() + &"line\n".repeat(100);
        let mut result = make_result();
        lint_structure(
            &content,
            Path::new("SKILL.md"),
            Path::new("."),
            "test",
            &mut result,
        );

        // Should have no structure warnings
        assert!(result.diagnostics.is_empty());
    }

    #[test]
    fn test_lint_heading_hierarchy_valid() {
        // Valid hierarchy: H1 -> H2 -> H3
        let content = "# Title\n\n## Section\n\n### Subsection\n";
        let mut result = make_result();
        lint_heading_hierarchy(content, Path::new("test.md"), Path::new("."), &mut result);

        assert!(
            result.diagnostics.is_empty(),
            "Valid hierarchy should have no warnings"
        );
    }

    #[test]
    fn test_lint_heading_hierarchy_skl204_first_not_h1() {
        // First heading is H2, not H1
        let content = "## Section\n\n### Subsection\n";
        let mut result = make_result();
        lint_heading_hierarchy(content, Path::new("test.md"), Path::new("."), &mut result);

        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].rule_id, "SKL204");
        assert!(
            result.diagnostics[0]
                .message
                .contains("first heading is H2")
        );
    }

    #[test]
    fn test_lint_heading_hierarchy_skl205_skipped_level() {
        // Skip from H1 to H3
        let content = "# Title\n\n### Subsection\n";
        let mut result = make_result();
        lint_heading_hierarchy(content, Path::new("test.md"), Path::new("."), &mut result);

        assert_eq!(result.diagnostics.len(), 1);
        assert_eq!(result.diagnostics[0].rule_id, "SKL205");
        assert!(
            result.diagnostics[0]
                .message
                .contains("skips from H1 to H3")
        );
    }

    #[test]
    fn test_lint_heading_hierarchy_skl205_multiple_skips() {
        // Multiple skipped levels
        let content = "# Title\n\n### Skip one\n\n##### Skip two\n";
        let mut result = make_result();
        lint_heading_hierarchy(content, Path::new("test.md"), Path::new("."), &mut result);

        // Should have 2 warnings for SKL205
        let skl205_count = result
            .diagnostics
            .iter()
            .filter(|d| d.rule_id == "SKL205")
            .count();
        assert_eq!(skl205_count, 2);
    }

    #[test]
    fn test_lint_heading_hierarchy_going_up_is_ok() {
        // Going up levels (H3 -> H1) is allowed
        let content = "# Title\n\n## Section\n\n### Sub\n\n# Another Title\n\n## Another Section\n";
        let mut result = make_result();
        lint_heading_hierarchy(content, Path::new("test.md"), Path::new("."), &mut result);

        assert!(
            result.diagnostics.is_empty(),
            "Going up levels should not trigger warnings"
        );
    }
}
