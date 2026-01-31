//! Outline command per [[RFC-0002:C-OUTLINE]]

use crate::config::get_cwd;
use crate::error::Result;
use crate::index;
use crate::logging::{LogEntry, get_run_id, init_log_db, log_access_with_fallback};
use crate::resolver::{ResolvedSkill, resolve_skill};
use crate::{Heading, OutputFormat, verbose};
use std::path::PathBuf;
use std::time::Instant;

use super::extract_headings;

/// Execute the outline command per [[RFC-0002:C-OUTLINE]].
///
/// Uses pre-built headings index if available (faster), otherwise falls back
/// to scanning .md files at runtime.
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

/// Try to get headings from pre-built index. Returns None if index unavailable.
fn try_outline_from_index(
    resolved: &ResolvedSkill,
    max_level: Option<usize>,
) -> Option<Vec<Heading>> {
    let conn =
        index::open_index(&resolved.runtime_dir, &resolved.source_dir, &resolved.name).ok()?;
    let entries = index::get_all_headings(&conn).ok()?;

    let mut headings: Vec<Heading> = entries
        .into_iter()
        .filter(|e| max_level.is_none_or(|max| e.level <= max))
        .map(|e| Heading {
            level: e.level,
            text: e.text,
            file: PathBuf::from(e.file),
            line_number: e.start_line,
        })
        .collect();

    // Index already ordered by file, start_line — but ensure sorting
    headings.sort_by(|a, b| (&a.file, a.line_number).cmp(&(&b.file, b.line_number)));

    Some(headings)
}

fn do_outline(
    resolved: &ResolvedSkill,
    max_level: Option<usize>,
    format: &OutputFormat,
) -> Result<String> {
    // Try index-based lookup first (much faster for built skills)
    let headings = match try_outline_from_index(resolved, max_level) {
        Some(h) => {
            verbose!("outline: using pre-built index");
            h
        }
        None => {
            verbose!("outline: falling back to filesystem scan");
            let mut h = extract_headings(&resolved.source_dir)?;
            if let Some(level) = max_level {
                h.retain(|h| h.level <= level);
            }
            h
        }
    };

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
