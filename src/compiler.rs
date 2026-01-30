//! Skill compiler per [[RFC-0001]] and RFC-0004

use crate::Heading;
use crate::config::ensure_dir;
use crate::error::{Result, SkillcError};
use crate::search;
use crate::verbose;
use chrono::Utc;
use lazy_regex::{Lazy, Regex, lazy_regex};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use walkdir::WalkDir;

/// Regex for parsing markdown headings (validated at compile time).
static HEADING_RE: Lazy<Regex> = lazy_regex!(r"^(#{1,6})\s+(.+)$");

/// Frontmatter extracted from SKILL.md
#[derive(Debug, Deserialize)]
pub struct Frontmatter {
    pub name: String,
    pub description: String,
}

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

    // Extract frontmatter
    let skill_md_content = fs::read_to_string(&skill_md_path)?;
    let frontmatter = extract_frontmatter(&skill_md_content)?;
    verbose!("build: skill name=\"{}\"", frontmatter.name);

    // Extract headings from all .md files
    let headings = extract_headings(source_dir)?;
    verbose!("build: extracted {} headings", headings.len());

    // Get list of .md files
    let md_files = list_md_files(source_dir)?;
    verbose!("build: found {} markdown files", md_files.len());

    // Compute source hash
    let source_hash = compute_source_hash(source_dir)?;
    verbose!("build: source_hash={}", &source_hash[..16]);

    // Generate stub
    let stub = generate_stub(&frontmatter, &headings);

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

/// Extract YAML frontmatter from SKILL.md content
fn extract_frontmatter(content: &str) -> Result<Frontmatter> {
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() || lines[0].trim() != "---" {
        return Err(SkillcError::InvalidFrontmatter(
            "File must start with ---".to_string(),
        ));
    }

    // Find closing ---
    let mut end_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_idx = Some(i);
            break;
        }
    }

    let end_idx = end_idx
        .ok_or_else(|| SkillcError::InvalidFrontmatter("No closing --- found".to_string()))?;

    let yaml_content = lines[1..end_idx].join("\n");
    let frontmatter: Frontmatter = serde_yaml::from_str(&yaml_content)?;

    // E011 per [[RFC-0005:C-CODES]]
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

/// Extract headings from all .md files in the source directory
fn extract_headings(source_dir: &Path) -> Result<Vec<Heading>> {
    let mut headings = Vec::new();

    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        let content = fs::read_to_string(entry.path())?;
        let relative_path = entry
            .path()
            .strip_prefix(source_dir)
            .map_err(|_| SkillcError::Internal("path does not start with source_dir".into()))?
            .to_path_buf();

        for (line_num, line) in content.lines().enumerate() {
            if let Some(caps) = HEADING_RE.captures(line) {
                let level = caps
                    .get(1)
                    .ok_or_else(|| SkillcError::Internal("regex group 1 missing".into()))?
                    .as_str()
                    .len();
                let text = caps
                    .get(2)
                    .ok_or_else(|| SkillcError::Internal("regex group 2 missing".into()))?
                    .as_str()
                    .to_string();
                headings.push(Heading {
                    level,
                    text,
                    file: relative_path.clone(),
                    line_number: line_num + 1, // 1-indexed
                });
            }
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
const MAX_STUB_LINES: usize = 100;

/// Generate the compiled stub per [[RFC-0001:C-STUB]]
///
/// Enforces the 100-line limit per [[RFC-0001:C-CONSTRAINTS]].
fn generate_stub(frontmatter: &Frontmatter, headings: &[Heading]) -> String {
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

    // MCP preference per [[RFC-0001:C-STUB]] (amended v0.3.0)
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

    // Calculate lines used so far (for 100-line limit)
    let header_lines = stub.lines().count();
    let available_lines = MAX_STUB_LINES.saturating_sub(header_lines + 3); // Reserve lines for section header and truncation indicator

    // Top sections (limited to 12 per [[RFC-0001:C-CONSTRAINTS]], and fit within line limit)
    stub.push_str("## Top Sections\n\n");
    let top_headings: Vec<_> = headings.iter().filter(|h| h.level <= 2).take(12).collect();

    let mut sections_added = 0;
    for heading in &top_headings {
        if sections_added >= available_lines {
            break;
        }
        let indent = "  ".repeat(heading.level.saturating_sub(1));
        stub.push_str(&format!("{}- {}\n", indent, heading.text));
        sections_added += 1;
    }

    let total_top_sections = headings.iter().filter(|h| h.level <= 2).count();
    if total_top_sections > sections_added || total_top_sections > 12 {
        stub.push_str("- ... (more sections available, use `skc outline`)\n");
    }

    stub
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter() {
        let content = r#"---
name: test-skill
description: A test skill
---

# Content here
"#;
        let fm = extract_frontmatter(content).expect("failed to extract frontmatter");
        assert_eq!(fm.name, "test-skill");
        assert_eq!(fm.description, "A test skill");
    }
}
