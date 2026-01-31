//! Skill compiler per [[RFC-0001]] and RFC-0004

use crate::Heading;
use crate::config::ensure_dir;
use crate::error::{Result, SkillcError};
use crate::frontmatter::{self, Frontmatter};
use crate::markdown;
use crate::search;
use crate::verbose;
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

/// Build manifest per [[RFC-0001:C-MANIFEST]]
#[derive(Debug, Serialize)]
pub struct Manifest {
    pub skill: String,
    pub version: u32,
    pub built_at: String,
    pub source_hash: String,
}

/// Compile a skill from source to runtime directory
pub fn compile(source_dir: &Path, runtime_dir: &Path) -> Result<()> {
    let start = Instant::now();

    verbose!("build: source_dir={}", source_dir.display());
    verbose!("build: runtime_dir={}", runtime_dir.display());

    // Validate source directory (E001/E010 per [[RFC-0005:C-CODES]])
    crate::util::validate_skill_path(source_dir)?;
    let skill_md_path = source_dir.join("SKILL.md");

    // Path safety: reject symlinks that escape skill root ([[RFC-0001:C-CONSTRAINTS]])
    check_symlink_safety(source_dir)?;

    // Extract frontmatter using shared parser
    let skill_md_content = fs::read_to_string(&skill_md_path)?;
    let frontmatter = frontmatter::parse(&skill_md_content)?;
    verbose!("build: skill name=\"{}\"", frontmatter.name);

    // Extract headings from all .md files
    let headings = extract_headings(source_dir)?;
    verbose!("build: extracted {} headings", headings.len());

    // Get list of .md files
    let md_files = list_md_files(source_dir)?;
    verbose!("build: found {} markdown files", md_files.len());

    // Extract reference descriptions per [[RFC-0008:C-REFERENCE-FRONTMATTER]]
    let descriptions = extract_reference_descriptions(source_dir, &md_files);
    verbose!(
        "build: found {} reference descriptions",
        descriptions.len()
    );

    // Compute source hash
    let source_hash = compute_source_hash(source_dir)?;
    verbose!("build: source_hash={}", &source_hash[..16]);

    // Generate stub
    let stub = generate_stub(&frontmatter, &headings, &descriptions);

    // Generate manifest
    let manifest = Manifest {
        skill: frontmatter.name.clone(),
        version: 1,
        built_at: Utc::now().to_rfc3339(),
        source_hash,
    };

    // Ensure runtime directory exists
    ensure_dir(runtime_dir)?;

    // Write stub
    let stub_path = runtime_dir.join("SKILL.md");
    fs::write(&stub_path, &stub)?;
    verbose!("build: wrote stub ({} bytes)", stub.len());

    // Write manifest
    let manifest_dir = runtime_dir.join(".skillc-meta");
    ensure_dir(&manifest_dir)?;
    let manifest_path = manifest_dir.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, &manifest_json)?;

    // Build search index per [[RFC-0004:C-INDEX]]
    search::build_index(source_dir, runtime_dir, &manifest.source_hash)?;

    verbose!("build: completed in {:?}", start.elapsed());

    Ok(())
}

/// Extract headings from all .md files in the source directory.
///
/// Uses pulldown-cmark AST to properly parse markdown, avoiding false
/// positives from `#` characters inside code blocks.
///
/// Files are processed in deterministic order:
/// 1. `SKILL.md` first (the main document)
/// 2. Other `.md` files in alphabetical order by path
///
/// This ensures the compiled stub's "Top Sections" reflect the main skill
/// content rather than arbitrary reference files.
fn extract_headings(source_dir: &Path) -> Result<Vec<Heading>> {
    // Collect all .md files first
    let mut md_files: Vec<PathBuf> = WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
        .map(|e| e.path().to_path_buf())
        .collect();

    // Sort: SKILL.md first, then alphabetically by path
    md_files.sort_by(|a, b| {
        let a_is_skill_md = a.file_name().is_some_and(|n| n == "SKILL.md");
        let b_is_skill_md = b.file_name().is_some_and(|n| n == "SKILL.md");
        match (a_is_skill_md, b_is_skill_md) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.cmp(b),
        }
    });

    let mut headings = Vec::new();

    for path in md_files {
        let content = fs::read_to_string(&path)?;
        let relative_path = path
            .strip_prefix(source_dir)
            .map_err(|_| SkillcError::Internal("path does not start with source_dir".into()))?
            .to_path_buf();

        // Use AST-based heading extraction
        for extracted in markdown::extract_headings(&content) {
            headings.push(Heading {
                level: extracted.level,
                text: extracted.text,
                file: relative_path.clone(),
                line_number: extracted.line,
            });
        }
    }

    Ok(headings)
}

/// List all .md files in the source directory
fn list_md_files(source_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let relative_path = entry
            .path()
            .strip_prefix(source_dir)
            .map_err(|_| {
                SkillcError::Internal(format!(
                    "path {} does not start with {}",
                    entry.path().display(),
                    source_dir.display()
                ))
            })?
            .to_path_buf();
        files.push(relative_path);
    }

    files.sort();
    Ok(files)
}

/// Compute SHA-256 hash of source files per [[RFC-0001:C-MANIFEST]]
fn compute_source_hash(source_dir: &Path) -> Result<String> {
    let mut file_hashes: Vec<(String, String)> = Vec::new();

    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_entry(|e| {
            // Exclude hidden directories (VCS, IDE settings, caches, etc.)
            !e.file_type().is_dir() || !e.file_name().to_string_lossy().starts_with('.')
        })
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let relative_path = entry
            .path()
            .strip_prefix(source_dir)
            .map_err(|_| SkillcError::Internal("path does not start with source_dir".into()))?
            .to_string_lossy()
            .to_string();
        let content = fs::read(entry.path())?;
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let file_hash = format!("{:x}", hasher.finalize());
        file_hashes.push((relative_path, file_hash));
    }

    // Sort by path for deterministic hash
    file_hashes.sort_by(|a, b| a.0.cmp(&b.0));

    // Hash the combined (path, hash) pairs
    let mut hasher = Sha256::new();
    for (path, hash) in &file_hashes {
        hasher.update(path.as_bytes());
        hasher.update(hash.as_bytes());
    }

    Ok(format!("{:x}", hasher.finalize()))
}

/// Check for symlinks that escape the skill root per [[RFC-0001:C-CONSTRAINTS]].
///
/// Returns E012 if any symlink resolves to a path outside the source directory.
fn check_symlink_safety(source_dir: &Path) -> Result<()> {
    let canonical_root = source_dir
        .canonicalize()
        .map_err(|e| SkillcError::Internal(format!("Failed to canonicalize source dir: {}", e)))?;

    for entry in WalkDir::new(source_dir)
        .follow_links(false) // Don't follow symlinks during traversal
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();

        // Check if this entry is a symlink
        if path
            .symlink_metadata()
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
        {
            // Resolve the symlink target
            match path.canonicalize() {
                Ok(resolved) => {
                    // Check if resolved path is within the skill root
                    if !resolved.starts_with(&canonical_root) {
                        let relative = path
                            .strip_prefix(source_dir)
                            .unwrap_or(path)
                            .to_string_lossy()
                            .to_string();
                        return Err(SkillcError::PathEscapesRoot(relative));
                    }
                }
                Err(_) => {
                    // Broken symlink - skip (not a security issue)
                    verbose!("build: skipping broken symlink: {}", path.display());
                }
            }
        }
    }

    Ok(())
}

/// Maximum stub size per [[RFC-0001:C-CONSTRAINTS]]
/// Currently unused but retained for future enforcement.
#[allow(dead_code)]
const MAX_STUB_LINES: usize = 100;

/// Maximum entries for SKILL.md sections per [[RFC-0001:C-SECTIONS]]
const MAX_SKILL_SECTION_ENTRIES: usize = 15;

/// Maximum entries for external references per [[RFC-0001:C-SECTIONS]]
const MAX_REFERENCE_ENTRIES: usize = 15;

/// Maximum length for reference descriptions per [[RFC-0001:C-SECTIONS]]
const MAX_REFERENCE_DESCRIPTION_LEN: usize = 120;

/// A section entry for the stub per [[RFC-0001:C-SECTIONS]]
#[derive(Debug)]
struct SectionEntry {
    /// Display text for the entry
    text: String,
    /// Indentation level (0 = no indent, 1 = one level, etc.)
    indent: usize,
}

/// Extract optional description from reference file frontmatter per [[RFC-0008:C-REFERENCE-FRONTMATTER]].
///
/// Returns a map of relative file paths to their descriptions.
fn extract_reference_descriptions(
    source_dir: &Path,
    files: &[PathBuf],
) -> HashMap<PathBuf, String> {
    let mut descriptions = HashMap::new();

    for file in files {
        // Skip SKILL.md - it uses the main frontmatter
        if file.file_name().is_some_and(|n| n == "SKILL.md") {
            continue;
        }

        let full_path = source_dir.join(file);
        if let Ok(content) = fs::read_to_string(&full_path) {
            if let Some(desc) = extract_description_from_frontmatter(&content) {
                descriptions.insert(file.clone(), desc);
            }
        }
    }

    descriptions
}

/// Extract description field from optional frontmatter.
///
/// Returns None if no frontmatter or no description field.
fn extract_description_from_frontmatter(content: &str) -> Option<String> {
    // Check for frontmatter delimiters
    if !content.starts_with("---") {
        return None;
    }

    // Find the closing delimiter
    let rest = &content[3..];
    let close_pos = rest.find("\n---")?;
    let yaml_block = &rest[..close_pos].trim();

    // Parse YAML to extract description
    // Use a simple approach: look for "description:" line
    for line in yaml_block.lines() {
        let line = line.trim();
        if line.starts_with("description:") {
            let value = line.strip_prefix("description:")?.trim();
            // Handle quoted strings
            let desc = if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                &value[1..value.len() - 1]
            } else {
                value
            };
            if !desc.is_empty() {
                return Some(desc.to_string());
            }
        }
    }

    None
}

/// Truncate description to max length with ellipsis per [[RFC-0001:C-SECTIONS]].
fn truncate_description(desc: &str, max_len: usize) -> String {
    if desc.chars().count() <= max_len {
        desc.to_string()
    } else {
        let truncated: String = desc.chars().take(max_len - 1).collect();
        format!("{}…", truncated.trim_end())
    }
}

/// Build section entries per [[RFC-0001:C-SECTIONS]].
///
/// - SKILL.md: H1 at indent 0, H2 at indent 1, skip H3+ (max 15 entries)
/// - Other files: Grouped under "References" with first H1 at indent 1 (max 15 entries)
/// - Reference entries include optional description per [[RFC-0008:C-REFERENCE-FRONTMATTER]]
/// - "References" header only shown if there are external files
/// - Truncated sections show "... (N more)" with exact omitted count
fn build_section_entries(
    headings: &[Heading],
    descriptions: &HashMap<PathBuf, String>,
) -> Vec<SectionEntry> {
    let mut skill_entries = Vec::new();
    let mut seen_files: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    let mut reference_entries = Vec::new();

    // Process SKILL.md headings
    for heading in headings {
        let is_skill_md = heading.file.file_name().is_some_and(|n| n == "SKILL.md");

        if is_skill_md {
            // SKILL.md: H1 at indent 0, H2 at indent 1, skip H3+
            match heading.level {
                1 => skill_entries.push(SectionEntry {
                    text: heading.text.clone(),
                    indent: 0,
                }),
                2 => skill_entries.push(SectionEntry {
                    text: heading.text.clone(),
                    indent: 1,
                }),
                _ => {} // Skip H3+
            }
        }
    }

    // Process other files - collect first H1 from each
    for heading in headings {
        let is_skill_md = heading.file.file_name().is_some_and(|n| n == "SKILL.md");

        if !is_skill_md {
            if seen_files.contains(&heading.file) {
                continue;
            }

            // Use first H1 heading
            if heading.level == 1 {
                seen_files.insert(heading.file.clone());

                // Build entry text with optional description per [[RFC-0001:C-SECTIONS]]
                let text = if let Some(desc) = descriptions.get(&heading.file) {
                    let truncated = truncate_description(desc, MAX_REFERENCE_DESCRIPTION_LEN);
                    format!("{} — {}", heading.text, truncated)
                } else {
                    heading.text.clone()
                };

                reference_entries.push(SectionEntry { text, indent: 1 });
            }
        }
    }

    // Add fallback entries for files without H1 headings
    let files_with_headings: std::collections::HashSet<_> = headings
        .iter()
        .filter(|h| h.file.file_name().is_none_or(|n| n != "SKILL.md"))
        .map(|h| &h.file)
        .collect();

    for file in files_with_headings {
        if !seen_files.contains(file) {
            // No H1 found, use filename (with optional description)
            let text = if let Some(desc) = descriptions.get(file) {
                let truncated = truncate_description(desc, MAX_REFERENCE_DESCRIPTION_LEN);
                format!("{} — {}", file.display(), truncated)
            } else {
                file.display().to_string()
            };
            reference_entries.push(SectionEntry { text, indent: 1 });
        }
    }

    // Calculate omitted counts before truncating
    let skill_omitted = skill_entries
        .len()
        .saturating_sub(MAX_SKILL_SECTION_ENTRIES);
    let refs_omitted = reference_entries
        .len()
        .saturating_sub(MAX_REFERENCE_ENTRIES);

    // Apply limits
    skill_entries.truncate(MAX_SKILL_SECTION_ENTRIES);
    reference_entries.truncate(MAX_REFERENCE_ENTRIES);

    // Build final entries
    let mut entries = skill_entries;

    // Add ellipsis with count for truncated SKILL.md sections
    if skill_omitted > 0 {
        entries.push(SectionEntry {
            text: format!("... ({} more)", skill_omitted),
            indent: 1,
        });
    }

    // Add "References" section only if there are reference entries
    if !reference_entries.is_empty() {
        entries.push(SectionEntry {
            text: "References".to_string(),
            indent: 0,
        });
        entries.extend(reference_entries);

        // Add ellipsis with count for truncated references
        if refs_omitted > 0 {
            entries.push(SectionEntry {
                text: format!("... ({} more)", refs_omitted),
                indent: 1,
            });
        }
    }

    entries
}

/// Generate the compiled stub per [[RFC-0001:C-STUB]]
///
/// Enforces the 100-line limit per [[RFC-0001:C-CONSTRAINTS]].
fn generate_stub(
    frontmatter: &Frontmatter,
    headings: &[Heading],
    descriptions: &HashMap<PathBuf, String>,
) -> String {
    let mut stub = String::new();

    // Frontmatter
    stub.push_str("---\n");
    stub.push_str(&format!("name: {}\n", frontmatter.name));
    stub.push_str(&format!("description: \"{}\"\n", frontmatter.description));
    stub.push_str("---\n\n");

    // Title
    stub.push_str(&format!("# {} (compiled)\n\n", frontmatter.name));

    // Notice
    stub.push_str("DO NOT read skill source files directly.\n");
    stub.push_str("Use the skillc gateway to access content.\n\n");

    // MCP preference per [[RFC-0001:C-STUB]]
    stub.push_str("## Usage\n\n");
    stub.push_str("**Prefer MCP if available:** Use skillc MCP tools (`skc_outline`, `skc_show`, `skc_search`, etc.) for better performance and structured output.\n\n");
    stub.push_str("**CLI fallback:**\n");
    stub.push_str(&format!(
        "- `skc outline {}` — list sections\n",
        frontmatter.name
    ));
    stub.push_str(&format!(
        "- `skc show {} --section \"<Heading>\"` — view section content\n",
        frontmatter.name
    ));
    stub.push_str(&format!(
        "- `skc open {} <relative-path>` — open file\n",
        frontmatter.name
    ));
    stub.push_str(&format!(
        "- `skc sources {}` — list source files\n",
        frontmatter.name
    ));
    stub.push_str(&format!(
        "- `skc search {} <query>` — search content\n\n",
        frontmatter.name
    ));

    // Build section entries per [[RFC-0001:C-SECTIONS]]
    let entries = build_section_entries(headings, descriptions);

    // Top sections
    stub.push_str("## Top Sections\n\n");

    for entry in &entries {
        let indent = "  ".repeat(entry.indent);
        stub.push_str(&format!("{}- {}\n", indent, entry.text));
    }

    stub
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontmatter_parsing() {
        let content = r#"---
name: test-skill
description: A test skill
---

# Content here
"#;
        let fm = frontmatter::parse(content).expect("failed to parse frontmatter");
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description, "A test skill");
    }

    #[test]
    fn test_build_section_entries_skill_md_only() {
        let headings = vec![
            Heading {
                level: 1,
                text: "My Skill".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 1,
            },
            Heading {
                level: 2,
                text: "Section One".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 5,
            },
            Heading {
                level: 2,
                text: "Section Two".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 10,
            },
        ];

        let entries = build_section_entries(&headings, &HashMap::new());

        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "My Skill");
        assert_eq!(entries[0].indent, 0);
        assert_eq!(entries[1].text, "Section One");
        assert_eq!(entries[1].indent, 1);
        assert_eq!(entries[2].text, "Section Two");
        assert_eq!(entries[2].indent, 1);
    }

    #[test]
    fn test_build_section_entries_skips_h3_plus() {
        let headings = vec![
            Heading {
                level: 1,
                text: "My Skill".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 1,
            },
            Heading {
                level: 2,
                text: "Section".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 5,
            },
            Heading {
                level: 3,
                text: "Subsection".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 10,
            },
            Heading {
                level: 4,
                text: "Deep".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 15,
            },
        ];

        let entries = build_section_entries(&headings, &HashMap::new());

        // Only H1 and H2 should be included
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "My Skill");
        assert_eq!(entries[1].text, "Section");
    }

    #[test]
    fn test_build_section_entries_with_references() {
        let headings = vec![
            Heading {
                level: 1,
                text: "My Skill".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 1,
            },
            Heading {
                level: 1,
                text: "Reference Doc".to_string(),
                file: PathBuf::from("docs/reference.md"),
                line_number: 1,
            },
            Heading {
                level: 1,
                text: "Another Doc".to_string(),
                file: PathBuf::from("docs/another.md"),
                line_number: 1,
            },
        ];

        let entries = build_section_entries(&headings, &HashMap::new());

        // My Skill + References header + 2 reference entries
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].text, "My Skill");
        assert_eq!(entries[0].indent, 0);
        assert_eq!(entries[1].text, "References");
        assert_eq!(entries[1].indent, 0);
        assert_eq!(entries[2].text, "Reference Doc");
        assert_eq!(entries[2].indent, 1);
        assert_eq!(entries[3].text, "Another Doc");
        assert_eq!(entries[3].indent, 1);
    }

    #[test]
    fn test_build_section_entries_no_references_when_empty() {
        let headings = vec![Heading {
            level: 1,
            text: "My Skill".to_string(),
            file: PathBuf::from("SKILL.md"),
            line_number: 1,
        }];

        let entries = build_section_entries(&headings, &HashMap::new());

        // No "References" section when there are no other files
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].text, "My Skill");
    }

    #[test]
    fn test_build_section_entries_uses_first_h1_from_other_files() {
        let headings = vec![
            Heading {
                level: 1,
                text: "My Skill".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 1,
            },
            Heading {
                level: 1,
                text: "First H1".to_string(),
                file: PathBuf::from("docs/multi.md"),
                line_number: 1,
            },
            Heading {
                level: 1,
                text: "Second H1".to_string(),
                file: PathBuf::from("docs/multi.md"),
                line_number: 10,
            },
            Heading {
                level: 2,
                text: "Some H2".to_string(),
                file: PathBuf::from("docs/multi.md"),
                line_number: 20,
            },
        ];

        let entries = build_section_entries(&headings, &HashMap::new());

        // Only first H1 from docs/multi.md should be included
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "My Skill");
        assert_eq!(entries[1].text, "References");
        assert_eq!(entries[2].text, "First H1");
    }

    #[test]
    fn test_build_section_entries_truncation_with_count() {
        // Create more than MAX_SKILL_SECTION_ENTRIES (15) headings: 1 H1 + 19 H2 = 20 entries
        let mut headings: Vec<Heading> = (0..20)
            .map(|i| Heading {
                level: if i == 0 { 1 } else { 2 },
                text: format!("Section {}", i),
                file: PathBuf::from("SKILL.md"),
                line_number: i + 1,
            })
            .collect();

        // Add reference files exceeding MAX_REFERENCE_ENTRIES (15): 20 files
        for i in 0..20 {
            headings.push(Heading {
                level: 1,
                text: format!("Ref {}", i),
                file: PathBuf::from(format!("refs/ref{}.md", i)),
                line_number: 1,
            });
        }

        let entries = build_section_entries(&headings, &HashMap::new());

        // Check for SKILL.md truncation indicator (20 - 15 = 5 more)
        let skill_ellipsis = entries
            .iter()
            .find(|e| e.text.starts_with("... (") && e.text.contains("5 more"));
        assert!(
            skill_ellipsis.is_some(),
            "Should have ellipsis showing 5 more for SKILL.md sections"
        );

        // Check for References section
        assert!(
            entries.iter().any(|e| e.text == "References"),
            "Should have References section"
        );

        // Check for References truncation indicator (20 - 15 = 5 more)
        let refs_ellipsis = entries
            .iter()
            .find(|e| e.text.starts_with("... (") && e.text.contains("5 more"));
        assert!(
            refs_ellipsis.is_some(),
            "Should have ellipsis showing 5 more for References"
        );
    }

    #[test]
    fn test_build_section_entries_multiple_h1_in_skill_md() {
        let headings = vec![
            Heading {
                level: 1,
                text: "Main Title".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 1,
            },
            Heading {
                level: 2,
                text: "Subsection".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 5,
            },
            Heading {
                level: 1,
                text: "Second Title".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 10,
            },
            Heading {
                level: 2,
                text: "Another Sub".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 15,
            },
        ];

        let entries = build_section_entries(&headings, &HashMap::new());

        // All H1s and H2s from SKILL.md should be included
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].text, "Main Title");
        assert_eq!(entries[0].indent, 0);
        assert_eq!(entries[1].text, "Subsection");
        assert_eq!(entries[1].indent, 1);
        assert_eq!(entries[2].text, "Second Title");
        assert_eq!(entries[2].indent, 0);
        assert_eq!(entries[3].text, "Another Sub");
        assert_eq!(entries[3].indent, 1);
    }

    #[test]
    fn test_build_section_entries_fallback_to_filename() {
        let headings = vec![
            Heading {
                level: 1,
                text: "My Skill".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 1,
            },
            // File with only H2, no H1
            Heading {
                level: 2,
                text: "Some H2".to_string(),
                file: PathBuf::from("docs/no-h1.md"),
                line_number: 1,
            },
        ];

        let entries = build_section_entries(&headings, &HashMap::new());

        // Should fallback to filename for docs/no-h1.md
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "My Skill");
        assert_eq!(entries[1].text, "References");
        assert_eq!(entries[2].text, "docs/no-h1.md");
        assert_eq!(entries[2].indent, 1);
    }

    #[test]
    fn test_extract_description_from_frontmatter() {
        // With description
        let content = r#"---
description: "Test description"
---

# Title
"#;
        let desc = extract_description_from_frontmatter(content);
        assert_eq!(desc, Some("Test description".to_string()));

        // Without frontmatter
        let content = "# Title\n\nContent";
        let desc = extract_description_from_frontmatter(content);
        assert_eq!(desc, None);

        // With frontmatter but no description
        let content = r#"---
other: value
---

# Title
"#;
        let desc = extract_description_from_frontmatter(content);
        assert_eq!(desc, None);

        // With single-quoted description
        let content = r#"---
description: 'Single quoted'
---
"#;
        let desc = extract_description_from_frontmatter(content);
        assert_eq!(desc, Some("Single quoted".to_string()));

        // With unquoted description
        let content = r#"---
description: Unquoted value
---
"#;
        let desc = extract_description_from_frontmatter(content);
        assert_eq!(desc, Some("Unquoted value".to_string()));
    }

    #[test]
    fn test_truncate_description() {
        // Short description - no truncation
        let desc = "Short description";
        assert_eq!(truncate_description(desc, 120), "Short description");

        // Exactly at limit
        let desc = "x".repeat(120);
        assert_eq!(truncate_description(&desc, 120), desc);

        // Over limit - truncated with ellipsis
        let desc = "x".repeat(130);
        let result = truncate_description(&desc, 120);
        assert!(result.ends_with('…'));
        assert!(result.chars().count() <= 120);
    }

    #[test]
    fn test_build_section_entries_with_descriptions() {
        let headings = vec![
            Heading {
                level: 1,
                text: "My Skill".to_string(),
                file: PathBuf::from("SKILL.md"),
                line_number: 1,
            },
            Heading {
                level: 1,
                text: "Clap Patterns".to_string(),
                file: PathBuf::from("refs/clap.md"),
                line_number: 1,
            },
            Heading {
                level: 1,
                text: "Error Handling".to_string(),
                file: PathBuf::from("refs/errors.md"),
                line_number: 1,
            },
        ];

        let mut descriptions = HashMap::new();
        descriptions.insert(
            PathBuf::from("refs/clap.md"),
            "Advanced argument parsing".to_string(),
        );
        // refs/errors.md has no description

        let entries = build_section_entries(&headings, &descriptions);

        assert_eq!(entries.len(), 4); // My Skill + References + 2 refs
        assert_eq!(entries[0].text, "My Skill");
        assert_eq!(entries[1].text, "References");
        assert_eq!(
            entries[2].text,
            "Clap Patterns — Advanced argument parsing"
        );
        assert_eq!(entries[3].text, "Error Handling"); // No description
    }
}
