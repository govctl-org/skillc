//! Skill listing functionality per [[RFC-0007:C-LIST]]

use crate::OutputFormat;
use crate::config::{find_project_root, global_skillc_dir, global_source_store};
use crate::error::{Result, SkillcError};
use crate::util::{project_skill_runtime_dir, project_skills_dir};
use comfy_table::{Cell, Color, ContentArrangement, Table};
use glob::Pattern;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Skill status per [[RFC-0007:C-LIST]]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillStatus {
    /// Runtime exists (and matches source if --check-obsolete)
    Normal,
    /// No runtime directory exists
    NotBuilt,
    /// Runtime exists but source hash differs from manifest (only with --check-obsolete)
    Obsolete,
}

impl std::fmt::Display for SkillStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillStatus::Normal => write!(f, "normal"),
            SkillStatus::NotBuilt => write!(f, "not-built"),
            SkillStatus::Obsolete => write!(f, "obsolete"),
        }
    }
}

/// Skill scope (project-local or global)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillScope {
    Project,
    Global,
}

impl std::fmt::Display for SkillScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SkillScope::Project => write!(f, "project"),
            SkillScope::Global => write!(f, "global"),
        }
    }
}

/// Information about a discovered skill
#[derive(Debug, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub scope: SkillScope,
    pub status: SkillStatus,
    pub source_path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_path: Option<PathBuf>,
}

/// Options for the list command
#[derive(Debug, Default)]
pub struct ListOptions {
    /// Filter by scope: Some(Project), Some(Global), or None for all
    pub scope: Option<SkillScope>,
    /// Filter by status: Some(status), or None for all
    pub status: Option<SkillStatus>,
    /// Maximum skills to return
    pub limit: Option<usize>,
    /// Filter by name pattern (glob)
    pub pattern: Option<String>,
    /// Enable obsolete runtime detection
    pub check_obsolete: bool,
}

/// Result of the list command
#[derive(Debug, Serialize)]
pub struct ListResult {
    pub skills: Vec<SkillInfo>,
    /// Total skills discovered (before filtering)
    pub total: usize,
}

/// List all skillc-managed skills per [[RFC-0007:C-LIST]]
pub fn list(options: &ListOptions) -> Result<ListResult> {
    let mut skills = Vec::new();

    // 1. Discover project-local skills (recursive-up search)
    if let Some(project_root) = find_project_root() {
        let skills_dir = project_skills_dir(&project_root);
        if skills_dir.is_dir() {
            discover_skills_in_dir(
                &skills_dir,
                SkillScope::Project,
                &project_root,
                options.check_obsolete,
                &mut skills,
            )?;
        }
    }

    // 2. Discover global skills
    if let Ok(global_skills_dir) = global_source_store()
        && global_skills_dir.is_dir()
    {
        discover_skills_in_dir(
            &global_skills_dir,
            SkillScope::Global,
            &global_skillc_dir()?,
            options.check_obsolete,
            &mut skills,
        )?;
    }

    // Sort: project before global, then alphabetical
    skills.sort_by(|a, b| match (&a.scope, &b.scope) {
        (SkillScope::Project, SkillScope::Global) => std::cmp::Ordering::Less,
        (SkillScope::Global, SkillScope::Project) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });

    let total = skills.len();

    // Apply filters
    let pattern = options.pattern.as_ref().and_then(|p| Pattern::new(p).ok());

    let filtered_skills: Vec<SkillInfo> = skills
        .into_iter()
        .filter(|s| {
            // Scope filter
            if let Some(scope) = options.scope
                && s.scope != scope
            {
                return false;
            }
            // Status filter
            if let Some(status) = options.status
                && s.status != status
            {
                return false;
            }
            // Pattern filter
            if let Some(ref pat) = pattern
                && !pat.matches(&s.name)
            {
                return false;
            }
            true
        })
        .collect();

    // Apply limit
    let skills = match options.limit {
        Some(limit) => filtered_skills.into_iter().take(limit).collect(),
        None => filtered_skills,
    };

    Ok(ListResult { skills, total })
}

/// Format list result for output
pub fn format_list(result: &ListResult, format: OutputFormat, verbose: bool) -> Result<String> {
    match format {
        OutputFormat::Text => format_text(result, verbose),
        OutputFormat::Json => serde_json::to_string_pretty(result)
            .map_err(|e| SkillcError::Internal(format!("JSON serialization failed: {}", e))),
    }
}

fn format_text(result: &ListResult, verbose: bool) -> Result<String> {
    if result.skills.is_empty() {
        return Ok("No skills found.".to_string());
    }

    let mut table = Table::new();
    table
        .load_preset(comfy_table::presets::NOTHING)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Header row
    if verbose {
        table.set_header(vec!["SKILL", "SCOPE", "STATUS", "SOURCE"]);
    } else {
        table.set_header(vec!["SKILL", "SCOPE", "STATUS"]);
    }

    // Data rows with colored status/scope
    for skill in &result.skills {
        let scope_cell = match skill.scope {
            SkillScope::Project => Cell::new("project").fg(Color::Cyan),
            SkillScope::Global => Cell::new("global").fg(Color::DarkGrey),
        };

        let status_cell = match skill.status {
            SkillStatus::Normal => Cell::new("normal").fg(Color::Green),
            SkillStatus::NotBuilt => Cell::new("not-built").fg(Color::Yellow),
            SkillStatus::Obsolete => Cell::new("obsolete").fg(Color::Red),
        };

        if verbose {
            table.add_row(vec![
                Cell::new(&skill.name),
                scope_cell,
                status_cell,
                Cell::new(skill.source_path.display().to_string()),
            ]);
        } else {
            table.add_row(vec![Cell::new(&skill.name), scope_cell, status_cell]);
        }
    }

    Ok(table.to_string())
}

/// Discover skills in a directory
fn discover_skills_in_dir(
    skills_dir: &Path,
    scope: SkillScope,
    context_root: &Path,
    check_obsolete: bool,
    skills: &mut Vec<SkillInfo>,
) -> Result<()> {
    for entry in fs::read_dir(skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }

        // Extract skill name from directory name
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        // Determine runtime path
        let runtime_path = match scope {
            SkillScope::Project => project_skill_runtime_dir(context_root, &name),
            SkillScope::Global => global_skillc_dir()?.join("runtime").join(&name),
        };

        // Determine status based on check_obsolete flag
        let (status, has_valid_runtime) = determine_status(&path, &runtime_path, check_obsolete)?;

        skills.push(SkillInfo {
            name,
            scope,
            status,
            source_path: path,
            runtime_path: if has_valid_runtime {
                Some(runtime_path)
            } else {
                None
            },
        });
    }

    Ok(())
}

/// Determine the status of a skill
///
/// Returns (status, has_valid_runtime) tuple.
/// - Without check_obsolete: just checks if runtime exists (fast)
/// - With check_obsolete: compares source hash with manifest (expensive)
fn determine_status(
    source_dir: &Path,
    runtime_dir: &Path,
    check_obsolete: bool,
) -> Result<(SkillStatus, bool)> {
    // Check if runtime exists with valid manifest
    // Manifest is inside .skillc-meta/ subdirectory of runtime
    let manifest_path = runtime_dir.join(".skillc-meta").join("manifest.json");
    if !runtime_dir.exists() || !manifest_path.exists() {
        return Ok((SkillStatus::NotBuilt, false));
    }

    // Runtime exists - if not checking obsolete, assume normal
    if !check_obsolete {
        return Ok((SkillStatus::Normal, true));
    }

    // Read manifest to get stored source_hash
    let manifest_content = fs::read_to_string(&manifest_path)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_content)
        .map_err(|e| SkillcError::Internal(format!("Failed to parse manifest: {}", e)))?;

    let stored_hash = match manifest.get("source_hash").and_then(|v| v.as_str()) {
        Some(h) => h,
        None => return Ok((SkillStatus::Obsolete, true)), // No hash in manifest = obsolete
    };

    // Compute current source hash (expensive operation)
    let current_hash = compute_source_hash(source_dir)?;

    if current_hash == stored_hash {
        Ok((SkillStatus::Normal, true))
    } else {
        Ok((SkillStatus::Obsolete, true))
    }
}

/// Compute SHA-256 hash of source files (same logic as compiler)
fn compute_source_hash(source_dir: &Path) -> Result<String> {
    let mut file_hashes: Vec<(String, String)> = Vec::new();

    for entry in WalkDir::new(source_dir)
        .into_iter()
        .filter_entry(|e| {
            // Exclude hidden directories
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_skill_status_display() {
        assert_eq!(format!("{}", SkillStatus::Normal), "normal");
        assert_eq!(format!("{}", SkillStatus::NotBuilt), "not-built");
        assert_eq!(format!("{}", SkillStatus::Obsolete), "obsolete");
    }

    #[test]
    fn test_skill_scope_display() {
        assert_eq!(format!("{}", SkillScope::Project), "project");
        assert_eq!(format!("{}", SkillScope::Global), "global");
    }

    #[test]
    fn test_determine_status_not_built() {
        let temp = TempDir::new().expect("create temp dir");
        let source_dir = temp.path().join("source");
        let runtime_dir = temp.path().join("runtime");

        fs::create_dir_all(&source_dir).expect("create test dir");
        fs::write(source_dir.join("SKILL.md"), "# Test").expect("test operation");

        let (status, has_runtime) =
            determine_status(&source_dir, &runtime_dir, true).expect("determine status");
        assert_eq!(status, SkillStatus::NotBuilt);
        assert!(!has_runtime);
    }
}
