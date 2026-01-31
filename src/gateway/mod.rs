//! Gateway commands per RFC-0002
//!
//! Provides read-only access to skill content through various commands.

mod open;
mod outline;
mod show;
mod sources;

pub use open::open;
pub use outline::outline;
pub use show::show;
pub use sources::sources;

use crate::error::{Result, SkillcError};
use crate::{Heading, markdown};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Extract headings from all .md files, sorted lexicographically by path.
///
/// Uses AST-based parsing to correctly skip headings inside code blocks.
/// Shared by outline and show fallback.
pub(crate) fn extract_headings(source_dir: &Path) -> Result<Vec<Heading>> {
    let mut headings = Vec::new();

    // Collect all .md files
    let mut md_files: Vec<PathBuf> = Vec::new();
    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let relative = entry
            .path()
            .strip_prefix(source_dir)
            .map_err(|_| SkillcError::Internal("path does not start with source_dir".into()))?
            .to_path_buf();
        md_files.push(relative);
    }

    // Sort lexicographically by path (bytewise ASCII order)
    md_files.sort();

    for file in md_files {
        let full_path = source_dir.join(&file);
        let content = fs::read_to_string(&full_path)?;

        // Use AST-based extraction to skip code blocks
        for extracted in markdown::extract_headings(&content) {
            headings.push(Heading {
                level: extracted.level,
                text: extracted.text,
                file: file.clone(),
                line_number: extracted.line,
            });
        }
    }

    Ok(headings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_skill() -> TempDir {
        let temp = TempDir::new().expect("failed to create temp dir");
        let skill_dir = temp.path();

        fs::write(
            skill_dir.join("SKILL.md"),
            r#"---
name: test-skill
description: A test skill
---

# Test Skill

## Getting Started

This is the getting started section.

### Prerequisites

You need these things.

## API Reference

API docs here.
"#,
        )
        .expect("failed to write SKILL.md");

        fs::create_dir_all(skill_dir.join("docs")).expect("failed to create docs dir");
        fs::write(
            skill_dir.join("docs").join("advanced.md"),
            r#"# Advanced Topics

## Performance

Performance tips here.
"#,
        )
        .expect("failed to write advanced.md");

        temp
    }

    #[test]
    fn test_extract_headings_sorted() {
        let temp = setup_test_skill();
        let headings = extract_headings(temp.path()).expect("failed to extract headings");

        // Should be sorted: SKILL.md before docs/advanced.md
        assert!(!headings.is_empty());
        assert_eq!(headings[0].file, PathBuf::from("SKILL.md"));
    }

    #[test]
    fn test_extract_headings_levels() {
        let temp = setup_test_skill();
        let headings = extract_headings(temp.path()).expect("failed to extract headings");

        // Check first few headings
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[0].text, "Test Skill");
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[1].text, "Getting Started");
    }
}
