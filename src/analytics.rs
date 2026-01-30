//! Usage analytics per RFC-0003

use crate::OutputFormat;
use crate::error::{Result, SkillcError};
use crate::resolver::resolve_skill;
use chrono::{DateTime, NaiveDate, Utc};
use clap::ValueEnum;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, ValueEnum, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum QueryType {
    Summary,
    Sections,
    Files,
    Commands,
    Projects,
    Errors,
}

pub struct StatsOptions {
    pub query: QueryType,
    pub format: OutputFormat,
    pub since: Option<String>,
    pub until: Option<String>,
    pub projects: Vec<String>,
}

#[derive(Clone)]
struct LogRow {
    timestamp: DateTime<Utc>,
    command: String,
    _skill_path: String, // Stored for potential future use
    cwd: String,
    args: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct FiltersOutput {
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    projects: Vec<String>,
}

#[derive(Serialize)]
struct PeriodOutput {
    start: Option<String>,
    end: Option<String>,
}

#[derive(Serialize)]
struct StatsResponse<T: Serialize> {
    skill: String,
    skill_path: String,
    query: QueryType,
    filters: FiltersOutput,
    period: PeriodOutput,
    data: T,
}

#[derive(Serialize, Deserialize)]
struct SummaryData {
    total_accesses: i64,
    unique_sections: i64,
    unique_files: i64,
    error_count: i64,
}

#[derive(Serialize, Deserialize)]
struct SectionEntry {
    section: String,
    file: String,
    count: i64,
}

#[derive(Serialize, Deserialize)]
struct FileEntry {
    file: String,
    count: i64,
}

#[derive(Serialize, Deserialize)]
struct ProjectEntry {
    project: String,
    count: i64,
}

#[derive(Serialize, Deserialize)]
struct ErrorEntry {
    target: String,
    command: String,
    error: String,
    count: i64,
}

/// Returns formatted stats output as a string.
pub fn stats(skill: &str, options: StatsOptions) -> Result<String> {
    let resolved = resolve_skill(skill)?;
    let db_path = resolved.runtime_dir.join(".skillc-meta").join("logs.db");

    let parsed_since = parse_datetime_option(options.since.as_deref())?;
    let parsed_until = parse_datetime_option(options.until.as_deref())?;
    let project_filters = canonicalize_projects(&options.projects)?;

    let filters_output = FiltersOutput {
        since: parsed_since,
        until: parsed_until,
        projects: project_filters
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
    };

    let rows = if db_path.exists() {
        let conn = Connection::open(&db_path)?;
        load_rows(
            &conn,
            &resolved.name,
            parsed_since,
            parsed_until,
            &project_filters,
        )?
    } else {
        Vec::new()
    };

    let period = compute_period(&rows);

    match options.query {
        QueryType::Summary => {
            let data = build_summary(&rows);
            format_response(
                &resolved.name,
                &resolved.source_dir.to_string_lossy(),
                QueryType::Summary,
                filters_output,
                period,
                data,
                &options.format,
            )
        }
        QueryType::Sections => {
            let data = build_sections(&rows);
            format_response(
                &resolved.name,
                &resolved.source_dir.to_string_lossy(),
                QueryType::Sections,
                filters_output,
                period,
                data,
                &options.format,
            )
        }
        QueryType::Files => {
            let data = build_files(&rows);
            format_response(
                &resolved.name,
                &resolved.source_dir.to_string_lossy(),
                QueryType::Files,
                filters_output,
                period,
                data,
                &options.format,
            )
        }
        QueryType::Commands => {
            let data = build_commands(&rows);
            format_response(
                &resolved.name,
                &resolved.source_dir.to_string_lossy(),
                QueryType::Commands,
                filters_output,
                period,
                data,
                &options.format,
            )
        }
        QueryType::Projects => {
            let data = build_projects(&rows);
            format_response(
                &resolved.name,
                &resolved.source_dir.to_string_lossy(),
                QueryType::Projects,
                filters_output,
                period,
                data,
                &options.format,
            )
        }
        QueryType::Errors => {
            let data = build_errors(&rows);
            format_response(
                &resolved.name,
                &resolved.source_dir.to_string_lossy(),
                QueryType::Errors,
                filters_output,
                period,
                data,
                &options.format,
            )
        }
    }
}

fn parse_datetime_option(value: Option<&str>) -> Result<Option<DateTime<Utc>>> {
    match value {
        Some(text) => Ok(Some(parse_datetime(text)?)),
        None => Ok(None),
    }
}

fn parse_datetime(input: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(input) {
        return Ok(dt.with_timezone(&Utc));
    }

    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .map_err(|_| SkillcError::InvalidDatetime(input.to_string()))?;
    let midnight = date
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| SkillcError::Internal("midnight should always be valid".into()))?;
    Ok(DateTime::<Utc>::from_naive_utc_and_offset(midnight, Utc))
}

fn canonicalize_projects(projects: &[String]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for project in projects {
        let path = PathBuf::from(project);
        let canonical = path.canonicalize().map_err(|_| {
            SkillcError::InvalidFilter(format!("project path not found: '{}'", project))
        })?;
        out.push(canonical);
    }
    Ok(out)
}

fn load_rows(
    conn: &Connection,
    skill_name: &str,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    projects: &[PathBuf],
) -> Result<Vec<LogRow>> {
    let mut stmt = conn.prepare(
        "SELECT timestamp, command, skill_path, cwd, args, error
         FROM access_log
         WHERE skill = ?1",
    )?;

    let rows = stmt
        .query_map([skill_name], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let mut out = Vec::new();
    for (timestamp, command, skill_path, cwd, args, error) in rows {
        let parsed_ts = DateTime::parse_from_rfc3339(&timestamp)
            .map_err(|_| SkillcError::InvalidDatetime(timestamp.clone()))?
            .with_timezone(&Utc);

        if let Some(since) = since
            && parsed_ts < since
        {
            continue;
        }
        if let Some(until) = until
            && parsed_ts > until
        {
            continue;
        }

        if !matches_project(&cwd, projects) {
            continue;
        }

        out.push(LogRow {
            timestamp: parsed_ts,
            command,
            _skill_path: skill_path,
            cwd,
            args,
            error,
        });
    }

    Ok(out)
}

fn matches_project(cwd: &str, projects: &[PathBuf]) -> bool {
    if projects.is_empty() {
        return true;
    }

    let cwd_path = Path::new(cwd);
    projects.iter().any(|project| cwd_path.starts_with(project))
}

fn compute_period(rows: &[LogRow]) -> PeriodOutput {
    let mut start: Option<DateTime<Utc>> = None;
    let mut end: Option<DateTime<Utc>> = None;

    for row in rows {
        start = match start {
            Some(current) => Some(current.min(row.timestamp)),
            None => Some(row.timestamp),
        };
        end = match end {
            Some(current) => Some(current.max(row.timestamp)),
            None => Some(row.timestamp),
        };
    }

    PeriodOutput {
        start: start.map(|dt| dt.to_rfc3339()),
        end: end.map(|dt| dt.to_rfc3339()),
    }
}

fn build_summary(rows: &[LogRow]) -> SummaryData {
    let mut sections = HashSet::new();
    let mut files = HashSet::new();
    let mut error_count = 0;

    for row in rows {
        if row.error.is_some() {
            error_count += 1;
        }

        if row.command == "show" {
            if let Some((section, file)) = parse_show_args(&row.args) {
                sections.insert(section);
                if let Some(file) = file {
                    files.insert(file);
                }
            }
        } else if row.command == "open"
            && let Some(path) = parse_open_args(&row.args)
        {
            files.insert(path);
        }
    }

    SummaryData {
        total_accesses: rows.len() as i64,
        unique_sections: sections.len() as i64,
        unique_files: files.len() as i64,
        error_count,
    }
}

fn build_sections(rows: &[LogRow]) -> Vec<SectionEntry> {
    let mut counts: HashMap<(String, String), i64> = HashMap::new();

    for row in rows.iter().filter(|r| r.command == "show") {
        if let Some((section, file)) = parse_show_args(&row.args)
            && let Some(file) = file
        {
            *counts.entry((section, file)).or_insert(0) += 1;
        }
    }

    let mut entries: Vec<SectionEntry> = counts
        .into_iter()
        .map(|((section, file), count)| SectionEntry {
            section,
            file,
            count,
        })
        .collect();

    entries.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.section.cmp(&b.section))
    });

    entries
}

fn build_files(rows: &[LogRow]) -> Vec<FileEntry> {
    let mut counts: HashMap<String, i64> = HashMap::new();

    for row in rows {
        if row.command == "open" {
            if let Some(path) = parse_open_args(&row.args) {
                *counts.entry(path).or_insert(0) += 1;
            }
        } else if row.command == "show"
            && let Some((_, file)) = parse_show_args(&row.args)
            && let Some(file) = file
        {
            *counts.entry(file).or_insert(0) += 1;
        }
    }

    let mut entries: Vec<FileEntry> = counts
        .into_iter()
        .map(|(file, count)| FileEntry { file, count })
        .collect();

    entries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.file.cmp(&b.file)));
    entries
}

fn build_commands(rows: &[LogRow]) -> BTreeMap<String, i64> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    for row in rows {
        *counts.entry(row.command.clone()).or_insert(0) += 1;
    }

    for known in ["outline", "show", "open"] {
        counts.entry(known.to_string()).or_insert(0);
    }

    counts.into_iter().collect()
}

fn build_projects(rows: &[LogRow]) -> Vec<ProjectEntry> {
    let mut counts: HashMap<String, i64> = HashMap::new();
    for row in rows {
        *counts.entry(row.cwd.clone()).or_insert(0) += 1;
    }

    let mut entries: Vec<ProjectEntry> = counts
        .into_iter()
        .map(|(project, count)| ProjectEntry { project, count })
        .collect();

    entries.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.project.cmp(&b.project))
    });
    entries
}

fn build_errors(rows: &[LogRow]) -> Vec<ErrorEntry> {
    let mut counts: HashMap<(String, String, String), i64> = HashMap::new();

    for row in rows.iter().filter(|r| r.error.is_some()) {
        let target = match row.command.as_str() {
            "show" => parse_show_args(&row.args)
                .map(|(section, file)| match file {
                    Some(file) => format!("{}#{}", file, section),
                    None => section,
                })
                .unwrap_or_else(|| "<unknown>".to_string()),
            "open" => parse_open_args(&row.args).unwrap_or_else(|| "<unknown>".to_string()),
            _ => "<unknown>".to_string(),
        };

        let error = row.error.clone().unwrap_or_else(|| "<unknown>".to_string());
        *counts
            .entry((target, row.command.clone(), error))
            .or_insert(0) += 1;
    }

    let mut entries: Vec<ErrorEntry> = counts
        .into_iter()
        .map(|((target, command, error), count)| ErrorEntry {
            target,
            command,
            error,
            count,
        })
        .collect();

    entries.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.command.cmp(&b.command))
            .then_with(|| a.target.cmp(&b.target))
            .then_with(|| a.error.cmp(&b.error))
    });

    entries
}

fn parse_show_args(args: &str) -> Option<(String, Option<String>)> {
    let parsed: serde_json::Value = serde_json::from_str(args).ok()?;
    let section = parsed.get("section")?.as_str()?.to_string();
    let file = parsed
        .get("file")
        .and_then(|value| value.as_str())
        .map(|value| value.to_string());
    Some((section, file))
}

fn parse_open_args(args: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(args).ok()?;
    parsed.get("path")?.as_str().map(|value| value.to_string())
}

fn format_response<T: Serialize>(
    skill: &str,
    skill_path: &str,
    query: QueryType,
    filters: FiltersOutput,
    period: PeriodOutput,
    data: T,
    format: &OutputFormat,
) -> Result<String> {
    let response = StatsResponse {
        skill: skill.to_string(),
        skill_path: skill_path.to_string(),
        query,
        filters,
        period,
        data,
    };

    match format {
        OutputFormat::Json => Ok(serde_json::to_string_pretty(&response)?),
        OutputFormat::Text => format_text(response),
    }
}

fn format_text<T: Serialize>(response: StatsResponse<T>) -> Result<String> {
    let mut lines = Vec::new();

    lines.push(format!("Skill: {}", response.skill));
    lines.push(format!("Path: {}", response.skill_path));
    lines.push(format!("Query: {:?}", response.query));
    lines.push(format!(
        "Filters: since={}, until={}, projects={}",
        response
            .filters
            .since
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "<none>".to_string()),
        response
            .filters
            .until
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| "<none>".to_string()),
        if response.filters.projects.is_empty() {
            "<none>".to_string()
        } else {
            response.filters.projects.join(", ")
        }
    ));
    lines.push(format!(
        "Period: start={}, end={}",
        response
            .period
            .start
            .unwrap_or_else(|| "<none>".to_string()),
        response.period.end.unwrap_or_else(|| "<none>".to_string())
    ));

    // Convert data to JSON value for type-specific formatting
    let data_value = serde_json::to_value(&response.data)
        .map_err(|e| SkillcError::Internal(format!("failed to serialize data: {}", e)))?;

    match response.query {
        QueryType::Summary => {
            let data: SummaryData = serde_json::from_value(data_value)
                .map_err(|e| SkillcError::Internal(format!("invalid SummaryData: {}", e)))?;
            lines.push(format!("Total accesses: {}", data.total_accesses));
            lines.push(format!("Unique sections: {}", data.unique_sections));
            lines.push(format!("Unique files: {}", data.unique_files));
            lines.push(format!("Error count: {}", data.error_count));
        }
        QueryType::Sections => {
            let entries: Vec<SectionEntry> = serde_json::from_value(data_value)
                .map_err(|e| SkillcError::Internal(format!("invalid SectionEntry: {}", e)))?;
            for entry in entries {
                lines.push(format!(
                    "{}\t{}\t{}",
                    entry.count, entry.file, entry.section
                ));
            }
        }
        QueryType::Files => {
            let entries: Vec<FileEntry> = serde_json::from_value(data_value)
                .map_err(|e| SkillcError::Internal(format!("invalid FileEntry: {}", e)))?;
            for entry in entries {
                lines.push(format!("{}\t{}", entry.count, entry.file));
            }
        }
        QueryType::Commands => {
            let map: BTreeMap<String, i64> = serde_json::from_value(data_value)
                .map_err(|e| SkillcError::Internal(format!("invalid command map: {}", e)))?;
            for (command, count) in map {
                lines.push(format!("{}\t{}", count, command));
            }
        }
        QueryType::Projects => {
            let entries: Vec<ProjectEntry> = serde_json::from_value(data_value)
                .map_err(|e| SkillcError::Internal(format!("invalid ProjectEntry: {}", e)))?;
            for entry in entries {
                lines.push(format!("{}\t{}", entry.count, entry.project));
            }
        }
        QueryType::Errors => {
            let entries: Vec<ErrorEntry> = serde_json::from_value(data_value)
                .map_err(|e| SkillcError::Internal(format!("invalid ErrorEntry: {}", e)))?;
            for entry in entries {
                lines.push(format!(
                    "{}\t{}\t{}\t{}",
                    entry.count, entry.command, entry.target, entry.error
                ));
            }
        }
    }

    Ok(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn row(command: &str, args: &str, error: Option<&str>) -> LogRow {
        LogRow {
            timestamp: Utc
                .with_ymd_and_hms(2026, 1, 26, 12, 0, 0)
                .single()
                .expect("valid datetime"),
            command: command.to_string(),
            _skill_path: "/tmp/skill".to_string(),
            cwd: "/work/project".to_string(),
            args: args.to_string(),
            error: error.map(|value| value.to_string()),
        }
    }

    #[test]
    fn test_parse_datetime_rfc3339() {
        let dt = parse_datetime("2026-01-26T12:34:56Z").expect("failed to parse datetime");
        assert_eq!(
            dt,
            Utc.with_ymd_and_hms(2026, 1, 26, 12, 34, 56)
                .single()
                .expect("valid datetime")
        );
    }

    #[test]
    fn test_parse_datetime_date_only() {
        let dt = parse_datetime("2026-01-26").expect("failed to parse date");
        assert_eq!(
            dt,
            Utc.with_ymd_and_hms(2026, 1, 26, 0, 0, 0)
                .single()
                .expect("valid datetime")
        );
    }

    #[test]
    fn test_build_summary_counts() {
        let rows = vec![
            row("show", r#"{"section":"Intro","file":"SKILL.md"}"#, None),
            row(
                "show",
                r#"{"section":"Intro","file":"SKILL.md"}"#,
                Some("miss"),
            ),
            row("open", r#"{"path":"docs/guide.md"}"#, None),
            row("outline", r#"{}"#, None),
        ];
        let summary = build_summary(&rows);
        assert_eq!(summary.total_accesses, 4);
        assert_eq!(summary.unique_sections, 1);
        assert_eq!(summary.unique_files, 2);
        assert_eq!(summary.error_count, 1);
    }

    #[test]
    fn test_build_sections_sorted() {
        let rows = vec![
            row("show", r#"{"section":"B","file":"b.md"}"#, None),
            row("show", r#"{"section":"A","file":"a.md"}"#, None),
            row("show", r#"{"section":"A","file":"a.md"}"#, None),
        ];
        let sections = build_sections(&rows);
        assert_eq!(sections[0].count, 2);
        assert_eq!(sections[0].file, "a.md");
        assert_eq!(sections[0].section, "A");
    }

    #[test]
    fn test_build_files_sorted() {
        let rows = vec![
            row("open", r#"{"path":"b.md"}"#, None),
            row("show", r#"{"section":"A","file":"a.md"}"#, None),
            row("show", r#"{"section":"B","file":"a.md"}"#, None),
        ];
        let files = build_files(&rows);
        assert_eq!(files[0].file, "a.md");
        assert_eq!(files[0].count, 2);
    }

    #[test]
    fn test_build_commands_includes_known() {
        let rows = vec![row("custom", "{}", None)];
        let commands = build_commands(&rows);
        assert_eq!(
            *commands.get("custom").expect("custom command should exist"),
            1
        );
        assert!(commands.contains_key("outline"));
        assert!(commands.contains_key("show"));
        assert!(commands.contains_key("open"));
    }

    #[test]
    fn test_build_errors_target_format() {
        let rows = vec![
            row(
                "show",
                r#"{"section":"Intro","file":"SKILL.md"}"#,
                Some("Section not found"),
            ),
            row(
                "open",
                r#"{"path":"docs/guide.md"}"#,
                Some("File not found"),
            ),
        ];
        let errors = build_errors(&rows);
        assert!(errors.iter().any(|entry| entry.target == "SKILL.md#Intro"));
        assert!(errors.iter().any(|entry| entry.target == "docs/guide.md"));
    }

    #[test]
    fn test_canonicalize_projects_invalid_path() {
        let projects = vec!["/nonexistent/path/123".to_string()];
        let result = canonicalize_projects(&projects);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("project path not found: '/nonexistent/path/123'")
        );
    }
}
