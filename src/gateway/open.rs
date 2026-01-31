//! Open command per [[RFC-0002:C-OPEN]]

use crate::config::get_cwd;
use crate::error::{Result, SkillcError};
use crate::logging::{LogEntry, get_run_id, init_log_db, log_access_with_fallback};
use crate::resolver::resolve_skill;
use crate::{OutputFormat, verbose};
use std::fs;
use std::time::Instant;

/// Execute the open command per [[RFC-0002:C-OPEN]].
///
/// Returns the contents of the specified file.
///
/// The `max_lines` parameter truncates output to the first n lines if specified.
pub fn open(
    skill: &str,
    path: &str,
    max_lines: Option<usize>,
    format: OutputFormat,
) -> Result<String> {
    let start = Instant::now();
    let resolved = resolve_skill(skill)?;
    let run_id = get_run_id();

    verbose!("open: path=\"{}\" max_lines={:?}", path, max_lines);
    verbose!("open: source_dir={}", resolved.source_dir.display());

    // Initialize logging
    let log_conn = init_log_db(&resolved.runtime_dir);

    let args = serde_json::json!({ "path": path, "max_lines": max_lines });

    let result = do_open(&resolved, path, max_lines, &format);

    verbose!("open: completed in {:?}", start.elapsed());

    // Log access (with automatic fallback for sandboxed environments)
    log_access_with_fallback(
        log_conn.as_ref(),
        &LogEntry {
            run_id,
            command: "open".to_string(),
            skill: resolved.name.clone(),
            skill_path: resolved.source_dir.to_string_lossy().to_string(),
            cwd: get_cwd(),
            args: args.to_string(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result
}

fn do_open(
    resolved: &crate::resolver::ResolvedSkill,
    path: &str,
    max_lines: Option<usize>,
    _format: &OutputFormat,
) -> Result<String> {
    // Validate path doesn't escape skill root
    if path.contains("..") {
        // Check if it actually escapes after canonicalization
        let full_path = resolved.source_dir.join(path);
        if let Ok(canonical) = full_path.canonicalize() {
            if !canonical.starts_with(&resolved.source_dir) {
                return Err(SkillcError::PathEscapesRoot(path.to_string()));
            }
        } else {
            return Err(SkillcError::PathEscapesRoot(path.to_string()));
        }
    }

    let file_path = resolved.source_dir.join(path);

    // Validate path after canonicalization
    if file_path.exists() {
        let canonical = file_path.canonicalize()?;
        if !canonical.starts_with(&resolved.source_dir) {
            return Err(SkillcError::PathEscapesRoot(path.to_string()));
        }
    }

    if !file_path.exists() {
        return Err(SkillcError::FileNotFound(path.to_string()));
    }

    if file_path.is_dir() {
        return Err(SkillcError::InvalidPath(
            "Path must be a file, not a directory".to_string(),
        ));
    }

    let content = fs::read_to_string(&file_path)?;

    // Apply max_lines truncation if specified
    if let Some(limit) = max_lines {
        let lines: Vec<&str> = content.lines().collect();
        if lines.len() > limit {
            let truncated: Vec<&str> = lines[..limit].to_vec();
            let remaining = lines.len() - limit;
            return Ok(format!(
                "{}\n... ({} more lines)",
                truncated.join("\n"),
                remaining
            ));
        }
    }

    Ok(content)
}
