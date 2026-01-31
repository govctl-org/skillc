//! Frontmatter parsing for SKILL.md files.
//!
//! This module provides shared frontmatter parsing used by both
//! the compiler and linter.

use crate::error::{Result, SkillcError};
use serde::Deserialize;
use std::collections::HashMap;

/// Frontmatter extracted from SKILL.md.
///
/// Contains all known frontmatter fields. Required fields (name, description)
/// are validated during parsing.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Frontmatter {
    /// Skill name (required)
    pub name: String,
    /// Skill description (required)
    pub description: String,
    /// Allowed tools (optional)
    #[serde(default)]
    pub allowed_tools: Option<String>,
    /// Capture unknown fields for lint validation
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// Raw frontmatter before validation.
///
/// Used for lenient parsing that doesn't fail on missing required fields,
/// allowing the linter to report specific diagnostics.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct RawFrontmatter {
    /// Skill name (may be missing)
    pub name: Option<String>,
    /// Skill description (may be missing)
    pub description: Option<String>,
    /// Allowed tools (optional)
    #[serde(default)]
    pub allowed_tools: Option<String>,
    /// Capture unknown fields for lint validation
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// Result of parsing frontmatter from markdown content.
#[derive(Debug)]
pub struct ParseResult {
    /// Parsed frontmatter (None if delimiters invalid)
    pub frontmatter: Option<RawFrontmatter>,
    /// Whether the `---` delimiters were valid
    pub valid_delimiters: bool,
    /// The raw YAML content between delimiters
    pub yaml_content: Option<String>,
}

/// Extract the YAML content between `---` delimiters.
///
/// Returns None if delimiters are missing or invalid.
fn extract_yaml_block(content: &str) -> Option<String> {
    // Strip opening delimiter (try CRLF first, then LF)
    let after_open = content
        .strip_prefix("---\r\n")
        .or_else(|| content.strip_prefix("---\n"))?;

    // Find closing delimiter
    let close_pos = after_open.find("\n---")?;

    Some(after_open[..close_pos].to_string())
}

/// Parse frontmatter leniently for linting.
///
/// Returns a ParseResult that allows inspection of parsing state
/// without failing on missing required fields.
pub fn parse_lenient(content: &str) -> ParseResult {
    let yaml_content = extract_yaml_block(content);

    let Some(yaml) = &yaml_content else {
        return ParseResult {
            frontmatter: None,
            valid_delimiters: false,
            yaml_content: None,
        };
    };

    let frontmatter = serde_yaml::from_str(yaml).ok();

    ParseResult {
        frontmatter,
        valid_delimiters: true,
        yaml_content,
    }
}

/// Parse and validate frontmatter strictly for compilation.
///
/// Returns an error if:
/// - Delimiters are missing or invalid
/// - Required fields (name, description) are missing or empty
pub fn parse(content: &str) -> Result<Frontmatter> {
    let yaml_content = extract_yaml_block(content).ok_or_else(|| {
        if !content.starts_with("---") {
            SkillcError::InvalidFrontmatter("File must start with ---".to_string())
        } else {
            SkillcError::InvalidFrontmatter("No closing --- found".to_string())
        }
    })?;

    let frontmatter: Frontmatter = serde_yaml::from_str(&yaml_content)?;

    // Validate required fields per [[RFC-0005:C-CODES]] E011
    if frontmatter.name.is_empty() {
        return Err(SkillcError::MissingFrontmatterField("name".to_string()));
    }
    if frontmatter.description.is_empty() {
        return Err(SkillcError::MissingFrontmatterField(
            "description".to_string(),
        ));
    }

    Ok(frontmatter)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_valid() {
        let content = r#"---
name: my-skill
description: A test skill
---

# Content
"#;
        let fm = parse(content).expect("parse frontmatter");
        assert_eq!(fm.name, "my-skill");
        assert_eq!(fm.description, "A test skill");
    }

    #[test]
    fn test_parse_with_allowed_tools() {
        let content = r#"---
name: my-skill
description: A test skill
allowed-tools: Read, Write
---
"#;
        let fm = parse(content).expect("parse frontmatter");
        assert_eq!(fm.allowed_tools, Some("Read, Write".to_string()));
    }

    #[test]
    fn test_parse_missing_open() {
        let content = "# No frontmatter";
        let result = parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_close() {
        let content = "---\nname: test\n# No close";
        let result = parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_name() {
        let content = r#"---
description: A test skill
---
"#;
        let result = parse(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_lenient_missing_name() {
        let content = r#"---
description: A test skill
---
"#;
        let result = parse_lenient(content);
        assert!(result.valid_delimiters);
        let fm = result.frontmatter.expect("should parse");
        assert!(fm.name.is_none());
        assert_eq!(fm.description, Some("A test skill".to_string()));
    }

    #[test]
    fn test_parse_lenient_invalid_delimiters() {
        let content = "# No frontmatter";
        let result = parse_lenient(content);
        assert!(!result.valid_delimiters);
        assert!(result.frontmatter.is_none());
    }

    #[test]
    fn test_parse_extra_fields() {
        let content = r#"---
name: my-skill
description: A test skill
unknown-field: value
---
"#;
        let fm = parse(content).expect("parse frontmatter");
        assert!(fm.extra.contains_key("unknown-field"));
    }
}
