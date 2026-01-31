//! Outline command per [[RFC-0002:C-OUTLINE]]

use crate::config::get_cwd;
use crate::error::Result;
use crate::logging::{LogEntry, get_run_id, init_log_db, log_access_with_fallback};
use crate::resolver::{ResolvedSkill, resolve_skill};
use crate::{OutputFormat, verbose};
use std::path::PathBuf;
use std::time::Instant;

use super::extract_headings;

/// Execute the outline command per [[RFC-0002:C-OUTLINE]].
///
/// Scans all .md files and extracts headings, sorted lexicographically by path.
/// Returns formatted output as a string.
///
/// The `max_level` parameter filters headings to include only those with level ≤ max_level.
pub fn outline(skill: &str, max_level: Option<usize>, format: OutputFormat) -> Result<String> {
    let start = Instant::now();
    let resolved = resolve_skill(skill)?;
    let run_id = get_run_id();

    verbose!(
        "outline: source_dir={} max_level={:?}",
        resolved.source_dir.display(),
        max_level
    );

    // Initialize logging (continue even if it fails)
    let log_conn = init_log_db(&resolved.runtime_dir);

    let args = serde_json::json!({ "level": max_level });
    let result = do_outline(&resolved, max_level, &format);

    verbose!("outline: completed in {:?}", start.elapsed());

    // Log access (with automatic fallback for sandboxed environments)
    log_access_with_fallback(
        log_conn.as_ref(),
        &LogEntry {
            run_id,
            command: "outline".to_string(),
            skill: resolved.name.clone(),
            skill_path: resolved.source_dir.to_string_lossy().to_string(),
            cwd: get_cwd(),
            args: args.to_string(),
            error: result.as_ref().err().map(|e| e.to_string()),
        },
    );

    result
}

fn do_outline(
    resolved: &ResolvedSkill,
    max_level: Option<usize>,
    format: &OutputFormat,
) -> Result<String> {
    let mut headings = extract_headings(&resolved.source_dir)?;

    // Filter by max level if specified
    if let Some(level) = max_level {
        headings.retain(|h| h.level <= level);
    }

    match format {
        OutputFormat::Json => {
            let json_headings: Vec<_> = headings
                .iter()
                .map(|h| {
                    serde_json::json!({
                        "level": h.level,
                        "heading": h.text,
                        "file": h.file.to_string_lossy()
                    })
                })
                .collect();
            Ok(serde_json::to_string_pretty(&json_headings)?)
        }
        OutputFormat::Text => {
            let mut output = String::new();
            let mut current_file: Option<&PathBuf> = None;

            for heading in &headings {
                if current_file != Some(&heading.file) {
                    if !output.is_empty() {
                        output.push('\n');
                    }
                    output.push_str(&heading.file.display().to_string());
                    output.push('\n');
                    current_file = Some(&heading.file);
                }

                // Indent based on level
                let indent = "  ".repeat(heading.level);
                let hashes = "#".repeat(heading.level);
                output.push_str(&format!("{}{} {}\n", indent, hashes, heading.text));
            }

            // Remove trailing newline if present
            if output.ends_with('\n') {
                output.pop();
            }

            Ok(output)
        }
    }
}
