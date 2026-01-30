//! Frontmatter lint rules (SKL100-SKL109) per [[RFC-0008:C-REGISTRY]]

use super::{Diagnostic, LintResult};
use crate::error::Result;
use lazy_regex::{Lazy, Regex, lazy_regex};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Regex for validating skill name format per [[RFC-0008:C-REGISTRY]] SKL102
static NAME_FORMAT_RE: Lazy<Regex> = lazy_regex!(r"^[a-z][a-z0-9-]*[a-z0-9]$|^[a-z]$");

/// Regex for detecting trigger phrases per [[RFC-0008:C-REGISTRY]] SKL108
static TRIGGER_RE: Lazy<Regex> =
    lazy_regex!(r"(?i)(use when|when to use|use for|triggers on|triggers:|activate when)");

/// Known frontmatter fields per [[RFC-0008:C-REGISTRY]] SKL109
const KNOWN_FIELDS: &[&str] = &["name", "description", "allowed-tools"];

/// Parse YAML frontmatter from SKILL.md content.
///
/// Returns (fields, valid_delimiters) where:
/// - fields: HashMap of field name -> value
/// - valid_delimiters: true if `---` delimiters are present and valid
pub fn parse_frontmatter(content: &str) -> Result<(HashMap<String, String>, bool)> {
    // Strip opening delimiter (try CRLF first, then LF)
    let after_open = content
        .strip_prefix("---\r\n")
        .or_else(|| content.strip_prefix("---\n"));

    let Some(after_open) = after_open else {
        return Ok((HashMap::new(), false));
    };

    // Find closing delimiter
    let Some(close_pos) = after_open.find("\n---") else {
        return Ok((HashMap::new(), false));
    };

    let yaml_content = &after_open[..close_pos];

    // Parse YAML (simple key: value parsing)
    let mut fields = HashMap::new();
    for line in yaml_content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            let value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            fields.insert(key, value);
        }
    }

    Ok((fields, true))
}

/// Lint frontmatter rules SKL100-SKL109
pub fn lint_frontmatter(
    content: &str,
    file_path: &Path,
    skill_path: &Path,
    dir_name: &str,
    result: &mut LintResult,
) -> Result<()> {
    let relative_path = file_path.strip_prefix(skill_path).unwrap_or(file_path);
    let (fields, valid_delimiters) = parse_frontmatter(content)?;

    // SKL100: frontmatter-valid
    if !valid_delimiters {
        let msg = if !content.starts_with("---") {
            "missing frontmatter: file does not start with ---"
        } else {
            "missing frontmatter: no closing --- found"
        };
        result.add(
            Diagnostic::error("SKL100", "frontmatter-valid", msg)
                .with_file(relative_path)
                .with_line(1),
        );
        // Cannot proceed with other frontmatter rules if delimiters are invalid
        return Ok(());
    }

    // SKL101: name-required
    let name = fields.get("name");
    if name.is_none() {
        result.add(
            Diagnostic::error("SKL101", "name-required", "missing required field 'name'")
                .with_file(relative_path),
        );
    }

    if let Some(name_val) = name {
        // SKL102: name-format
        if !name_val.is_empty() && !NAME_FORMAT_RE.is_match(name_val) {
            result.add(
                Diagnostic::error(
                    "SKL102",
                    "name-format",
                    format!(
                        "name '{}' contains invalid characters (must be lowercase a-z, 0-9, hyphens)",
                        name_val
                    ),
                )
                .with_file(relative_path),
            );
        }

        // SKL103: name-length
        if name_val.is_empty() {
            result.add(
                Diagnostic::error("SKL103", "name-length", "name is empty")
                    .with_file(relative_path),
            );
        } else if name_val.len() > 64 {
            result.add(
                Diagnostic::error(
                    "SKL103",
                    "name-length",
                    format!("name exceeds 64 characters ({} chars)", name_val.len()),
                )
                .with_file(relative_path),
            );
        }

        // SKL104: name-match-dir
        if !name_val.is_empty() && name_val != dir_name {
            result.add(
                Diagnostic::warning(
                    "SKL104",
                    "name-match-dir",
                    format!(
                        "name '{}' does not match directory '{}'",
                        name_val, dir_name
                    ),
                )
                .with_file(relative_path),
            );
        }
    }

    // SKL105: description-required
    let description = fields.get("description");
    if description.is_none() {
        result.add(
            Diagnostic::error(
                "SKL105",
                "description-required",
                "missing required field 'description'",
            )
            .with_file(relative_path),
        );
    }

    if let Some(desc_val) = description {
        // SKL106: description-nonempty
        if desc_val.trim().is_empty() {
            result.add(
                Diagnostic::error("SKL106", "description-nonempty", "description is empty")
                    .with_file(relative_path),
            );
        }

        // SKL107: description-length
        if desc_val.len() > 1024 {
            result.add(
                Diagnostic::warning(
                    "SKL107",
                    "description-length",
                    format!(
                        "description exceeds 1024 characters ({} chars)",
                        desc_val.len()
                    ),
                )
                .with_file(relative_path),
            );
        }

        // SKL108: description-triggers
        if !desc_val.trim().is_empty() && !TRIGGER_RE.is_match(desc_val) {
            result.add(
                Diagnostic::warning(
                    "SKL108",
                    "description-triggers",
                    "description lacks activation trigger (missing 'Use when' or similar)",
                )
                .with_file(relative_path),
            );
        }
    }

    // SKL109: frontmatter-known
    let known_set: HashSet<&str> = KNOWN_FIELDS.iter().copied().collect();
    for key in fields.keys() {
        if !known_set.contains(key.as_str()) {
            result.add(
                Diagnostic::warning(
                    "SKL109",
                    "frontmatter-known",
                    format!("unknown frontmatter field '{}'", key),
                )
                .with_file(relative_path),
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_valid() {
        let content = r#"---
name: my-skill
description: "A skill"
---

# Content
"#;
        let (fields, valid) = parse_frontmatter(content).expect("parse frontmatter");
        assert!(valid);
        assert_eq!(fields.get("name"), Some(&"my-skill".to_string()));
        assert_eq!(fields.get("description"), Some(&"A skill".to_string()));
    }

    #[test]
    fn test_parse_frontmatter_missing_open() {
        let content = "# No frontmatter";
        let (_, valid) = parse_frontmatter(content).expect("parse frontmatter");
        assert!(!valid);
    }

    #[test]
    fn test_parse_frontmatter_missing_close() {
        let content = "---\nname: test\n# No close";
        let (_, valid) = parse_frontmatter(content).expect("parse frontmatter");
        assert!(!valid);
    }

    #[test]
    fn test_name_format_regex() {
        // Valid names
        assert!(NAME_FORMAT_RE.is_match("a"));
        assert!(NAME_FORMAT_RE.is_match("ab"));
        assert!(NAME_FORMAT_RE.is_match("my-skill"));
        assert!(NAME_FORMAT_RE.is_match("skill123"));
        assert!(NAME_FORMAT_RE.is_match("my-skill-2"));
        assert!(NAME_FORMAT_RE.is_match("a1"));

        // Invalid names
        assert!(!NAME_FORMAT_RE.is_match(""));
        assert!(!NAME_FORMAT_RE.is_match("My-Skill")); // uppercase
        assert!(!NAME_FORMAT_RE.is_match("-skill")); // leading hyphen
        assert!(!NAME_FORMAT_RE.is_match("skill-")); // trailing hyphen
        assert!(!NAME_FORMAT_RE.is_match("skill_name")); // underscore
        assert!(!NAME_FORMAT_RE.is_match("1skill")); // starts with number
    }

    #[test]
    fn test_trigger_regex() {
        // Should match
        assert!(TRIGGER_RE.is_match("Use when working with files"));
        assert!(TRIGGER_RE.is_match("use when testing"));
        assert!(TRIGGER_RE.is_match("When to use: for testing"));
        assert!(TRIGGER_RE.is_match("Triggers on file changes"));
        assert!(TRIGGER_RE.is_match("triggers: file changes"));
        assert!(TRIGGER_RE.is_match("Activate when needed"));
        assert!(TRIGGER_RE.is_match("Use for testing"));

        // Should not match
        assert!(!TRIGGER_RE.is_match("A simple description"));
        assert!(!TRIGGER_RE.is_match("This helps with testing"));
    }

    #[test]
    fn test_lint_missing_name() {
        let content = r#"---
description: "A skill"
---
"#;
        let mut result = LintResult::new("test".to_string(), std::path::PathBuf::new());
        lint_frontmatter(
            content,
            std::path::Path::new("SKILL.md"),
            std::path::Path::new("."),
            "test",
            &mut result,
        )
        .expect("write test file");

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL101"));
    }

    #[test]
    fn test_lint_invalid_name_format() {
        let content = r#"---
name: My-Skill
description: "A skill"
---
"#;
        let mut result = LintResult::new("test".to_string(), std::path::PathBuf::new());
        lint_frontmatter(
            content,
            std::path::Path::new("SKILL.md"),
            std::path::Path::new("."),
            "test",
            &mut result,
        )
        .expect("write test file");

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL102"));
    }

    #[test]
    fn test_lint_name_too_long() {
        let long_name = "a".repeat(65);
        let content = format!(
            r#"---
name: {}
description: "A skill"
---
"#,
            long_name
        );
        let mut result = LintResult::new("test".to_string(), std::path::PathBuf::new());
        lint_frontmatter(
            &content,
            std::path::Path::new("SKILL.md"),
            std::path::Path::new("."),
            "test",
            &mut result,
        )
        .expect("write test file");

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL103"));
    }

    #[test]
    fn test_lint_missing_trigger() {
        let content = r#"---
name: test
description: "A simple description without trigger"
---
"#;
        let mut result = LintResult::new("test".to_string(), std::path::PathBuf::new());
        lint_frontmatter(
            content,
            std::path::Path::new("SKILL.md"),
            std::path::Path::new("."),
            "test",
            &mut result,
        )
        .expect("write test file");

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL108"));
    }

    #[test]
    fn test_lint_unknown_field() {
        let content = r#"---
name: test
description: "Use when testing"
unknown_field: value
---
"#;
        let mut result = LintResult::new("test".to_string(), std::path::PathBuf::new());
        lint_frontmatter(
            content,
            std::path::Path::new("SKILL.md"),
            std::path::Path::new("."),
            "test",
            &mut result,
        )
        .expect("write test file");

        assert!(result.diagnostics.iter().any(|d| d.rule_id == "SKL109"));
    }

    #[test]
    fn test_lint_allowed_tools_not_unknown() {
        let content = r#"---
name: test
description: "Use when testing"
allowed-tools: Read, Write
---
"#;
        let mut result = LintResult::new("test".to_string(), std::path::PathBuf::new());
        lint_frontmatter(
            content,
            std::path::Path::new("SKILL.md"),
            std::path::Path::new("."),
            "test",
            &mut result,
        )
        .expect("write test file");

        // Should NOT have SKL109 warning for allowed-tools
        assert!(!result.diagnostics.iter().any(|d| d.rule_id == "SKL109"));
    }
}
