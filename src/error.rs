//! Diagnostic types per RFC-0005: Diagnostic Code Registry
//!
//! All error and warning messages are canonical and defined in [[RFC-0005:C-CODES]].
//! - Errors appear as `error[EXXX]: message` and exit with code 1
//! - Warnings appear as `warning[WXXX]: message` and do not affect exit code
//!
//! Design: SSOT — codes defined once in enums, messages once in `message()` methods.

use crossterm::style::Stylize;
use std::fmt;
use std::io::IsTerminal;

/// Error codes per [[RFC-0005:C-CODES]].
///
/// These codes appear in user-facing error messages and are used for
/// documentation and issue tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// E001: Skill resolution failed (no matching directory in any store)
    E001,
    /// E002: Index is missing, corrupt, or stale
    E002,
    /// E003: Index filename exists but belongs to different skill
    E003,
    /// E004: Search query is empty or whitespace-only
    E004,
    /// E010: Directory exists but lacks SKILL.md
    E010,
    /// E011: SKILL.md lacks required `name` or `description`
    E011,
    /// E012: Symlink or path traversal would escape skill directory
    E012,
    /// E020: Gateway show command found no matching heading
    E020,
    /// E021: Gateway open command target does not exist
    E021,
    /// E022: Gateway sources --dir target does not exist
    E022,
    /// E030: Stats command received unknown query type
    E030,
    /// E031: Stats command received malformed filter value
    E031,
    /// E040: Sync command found no fallback logs to sync
    E040,
    /// E041: Sync command cannot write to primary runtime directory
    E041,
    /// E042: Sync command cannot read from fallback log database
    E042,
    /// E050: Init command target already has SKILL.md
    E050,
    /// E100: CLI parsing failed (unknown flag, missing value, etc.)
    E100,
    /// E999: Internal error (IO, database, parsing failures)
    E999,
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Warning codes per [[RFC-0005:C-CODES]].
///
/// These codes appear in user-facing warning messages and are used for
/// documentation and issue tracking. Warnings do not affect exit code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningCode {
    /// W001: Gateway show found multiple headings matching query
    W001,
    /// W002: Access logging failed, using fallback or disabled
    W002,
    /// W003: Local fallback logs exist and are older than threshold
    W003,
}

impl fmt::Display for WarningCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Skillc warning type with canonical messages per [[RFC-0005:C-CODES]].
///
/// All warning messages include the code: `warning[WXXX]: message`.
/// Warnings are printed to stderr but do not cause command failure.
#[derive(Debug)]
pub enum SkillcWarning {
    /// W001: Multiple matches for section
    MultipleMatches(String),
    /// W002: Logging disabled
    LoggingDisabled,
    /// W003: Stale local logs
    StaleLogs(String),
}

impl SkillcWarning {
    /// Returns the warning code (single source of truth).
    pub fn code(&self) -> WarningCode {
        match self {
            SkillcWarning::MultipleMatches(_) => WarningCode::W001,
            SkillcWarning::LoggingDisabled => WarningCode::W002,
            SkillcWarning::StaleLogs(_) => WarningCode::W003,
        }
    }

    /// Returns the warning message (single source of truth).
    fn message(&self) -> String {
        match self {
            SkillcWarning::MultipleMatches(s) => {
                format!("multiple matches for '{}'; showing first", s)
            }
            SkillcWarning::LoggingDisabled => {
                "logging disabled; run 'skc sync' after session to merge logs".to_string()
            }
            SkillcWarning::StaleLogs(s) => {
                format!("stale local logs for '{}'; run 'skc sync' to upload", s)
            }
        }
    }

    /// Emit this warning to stderr.
    pub fn emit(&self) {
        eprintln!("warning[{}]: {}", self.code(), self.message());
    }
}

impl fmt::Display for SkillcWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = format!("warning[{}]", self.code());
        if std::io::stderr().is_terminal() {
            write!(f, "{}: {}", prefix.yellow().bold(), self.message())
        } else {
            write!(f, "{}: {}", prefix, self.message())
        }
    }
}

/// Skillc error type with canonical messages per [[RFC-0005:C-CODES]].
///
/// All error messages include the error code: `error[EXXX]: message`.
/// Implementations MUST print these exact messages to stderr.
#[derive(Debug)]
pub enum SkillcError {
    // E001–E010: Unified errors (skill resolution, index state, query, path)
    SkillNotFound(String),
    IndexUnusable(String),
    IndexHashCollision(String),
    EmptyQuery,
    NotAValidSkill(String),

    // E011–E019: Compilation errors (RFC-0001)
    MissingFrontmatterField(String),
    InvalidFrontmatter(String),

    // E012: Path escape (unified)
    PathEscapesRoot(String),

    // E020–E029: Gateway errors (RFC-0002)
    SectionNotFound(String),
    FileNotFound(String),
    DirectoryNotFound(String),

    // E030–E039: Analytics errors (RFC-0003)
    InvalidQueryType(String),
    InvalidFilter(String),

    // E040–E049: Sync errors (RFC-0007)
    NoLocalLogs,
    SyncDestNotWritable(String, String),
    SyncSourceNotReadable(String, String),

    // E050–E059: Scaffolding errors (RFC-0006)
    SkillAlreadyExists(String),

    // E100–E199: CLI parsing errors
    InvalidOption(String),

    // E999: Internal errors (with message)
    Internal(String),

    // E999: Internal errors
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
    Json(serde_json::Error),
    Sql(rusqlite::Error),
    InvalidDatetime(String),
    InvalidPath(String),
}

impl SkillcError {
    /// Returns the error code (single source of truth).
    pub fn code(&self) -> ErrorCode {
        match self {
            SkillcError::SkillNotFound(_) => ErrorCode::E001,
            SkillcError::IndexUnusable(_) => ErrorCode::E002,
            SkillcError::IndexHashCollision(_) => ErrorCode::E003,
            SkillcError::EmptyQuery => ErrorCode::E004,
            SkillcError::NotAValidSkill(_) => ErrorCode::E010,
            SkillcError::MissingFrontmatterField(_) => ErrorCode::E011,
            SkillcError::InvalidFrontmatter(_) => ErrorCode::E011,
            SkillcError::PathEscapesRoot(_) => ErrorCode::E012,
            SkillcError::SectionNotFound(_) => ErrorCode::E020,
            SkillcError::FileNotFound(_) => ErrorCode::E021,
            SkillcError::DirectoryNotFound(_) => ErrorCode::E022,
            SkillcError::InvalidQueryType(_) => ErrorCode::E030,
            SkillcError::InvalidFilter(_) => ErrorCode::E031,
            SkillcError::InvalidDatetime(_) => ErrorCode::E031,
            SkillcError::NoLocalLogs => ErrorCode::E040,
            SkillcError::SyncDestNotWritable(_, _) => ErrorCode::E041,
            SkillcError::SyncSourceNotReadable(_, _) => ErrorCode::E042,
            SkillcError::SkillAlreadyExists(_) => ErrorCode::E050,
            SkillcError::InvalidOption(_) => ErrorCode::E100,
            // E999: Internal errors
            SkillcError::Io(_) => ErrorCode::E999,
            SkillcError::Yaml(_) => ErrorCode::E999,
            SkillcError::Json(_) => ErrorCode::E999,
            SkillcError::Sql(_) => ErrorCode::E999,
            SkillcError::InvalidPath(_) => ErrorCode::E999,
            SkillcError::Internal(_) => ErrorCode::E999,
        }
    }

    /// Returns the error message (single source of truth).
    fn message(&self) -> String {
        match self {
            SkillcError::SkillNotFound(s) => format!("skill '{}' not found", s),
            SkillcError::IndexUnusable(s) => {
                format!("search index unusable; run 'skc build {}' to rebuild", s)
            }
            SkillcError::IndexHashCollision(s) => {
                format!(
                    "index hash collision; delete .skillc-meta/search-{}.db and rebuild",
                    s
                )
            }
            SkillcError::EmptyQuery => "empty query".to_string(),
            SkillcError::NotAValidSkill(s) => {
                format!("not a valid skill: '{}' (missing SKILL.md)", s)
            }
            SkillcError::MissingFrontmatterField(s) => {
                format!("missing frontmatter field '{}' in SKILL.md", s)
            }
            SkillcError::InvalidFrontmatter(s) => {
                format!("invalid frontmatter in SKILL.md: {}", s)
            }
            SkillcError::PathEscapesRoot(s) => format!("path escapes skill root: '{}'", s),
            SkillcError::SectionNotFound(s) => format!("section not found: '{}'", s),
            SkillcError::FileNotFound(s) => format!("file not found: '{}'", s),
            SkillcError::DirectoryNotFound(s) => format!("directory not found: '{}'", s),
            SkillcError::InvalidQueryType(s) => format!("invalid query type: '{}'", s),
            SkillcError::InvalidFilter(s) => format!("invalid filter: '{}'", s),
            SkillcError::InvalidDatetime(s) => format!("invalid filter: '{}'", s),
            SkillcError::NoLocalLogs => "no local logs found".to_string(),
            SkillcError::SyncDestNotWritable(path, msg) => {
                format!("sync destination not writable: '{}' ({})", path, msg)
            }
            SkillcError::SyncSourceNotReadable(path, msg) => {
                format!("sync source not readable: '{}' ({})", path, msg)
            }
            SkillcError::SkillAlreadyExists(s) => format!("skill '{}' already exists", s),
            SkillcError::InvalidOption(s) => format!("invalid option: '{}'", s),
            // Internal errors: pass through the underlying message
            SkillcError::Io(e) => e.to_string(),
            SkillcError::Yaml(e) => e.to_string(),
            SkillcError::Json(e) => e.to_string(),
            SkillcError::Sql(e) => e.to_string(),
            SkillcError::InvalidPath(s) => s.clone(),
            SkillcError::Internal(s) => s.clone(),
        }
    }
}

impl fmt::Display for SkillcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let prefix = format!("error[{}]", self.code());
        if std::io::stderr().is_terminal() {
            write!(f, "{}: {}", prefix.red().bold(), self.message())
        } else {
            write!(f, "{}: {}", prefix, self.message())
        }
    }
}

impl std::error::Error for SkillcError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SkillcError::Io(e) => Some(e),
            SkillcError::Yaml(e) => Some(e),
            SkillcError::Json(e) => Some(e),
            SkillcError::Sql(e) => Some(e),
            _ => None,
        }
    }
}

// Manual From impls (replacing thiserror's #[from])
impl From<std::io::Error> for SkillcError {
    fn from(e: std::io::Error) -> Self {
        SkillcError::Io(e)
    }
}

impl From<serde_yaml::Error> for SkillcError {
    fn from(e: serde_yaml::Error) -> Self {
        SkillcError::Yaml(e)
    }
}

impl From<serde_json::Error> for SkillcError {
    fn from(e: serde_json::Error) -> Self {
        SkillcError::Json(e)
    }
}

impl From<rusqlite::Error> for SkillcError {
    fn from(e: rusqlite::Error) -> Self {
        SkillcError::Sql(e)
    }
}

pub type Result<T> = std::result::Result<T, SkillcError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_display() {
        assert_eq!(ErrorCode::E001.to_string(), "E001");
        assert_eq!(ErrorCode::E010.to_string(), "E010");
        assert_eq!(ErrorCode::E999.to_string(), "E999");
    }

    #[test]
    fn test_warning_code_display() {
        assert_eq!(WarningCode::W001.to_string(), "W001");
        assert_eq!(WarningCode::W002.to_string(), "W002");
        assert_eq!(WarningCode::W003.to_string(), "W003");
    }

    #[test]
    fn test_skillc_error_display() {
        // Tests check for content presence since output includes ANSI color codes
        let err = SkillcError::SkillNotFound("my-skill".to_string());
        let s = err.to_string();
        assert!(s.contains("error[E001]"));
        assert!(s.contains("skill 'my-skill' not found"));

        let err = SkillcError::NotAValidSkill("/path/to/skill".to_string());
        let s = err.to_string();
        assert!(s.contains("error[E010]"));
        assert!(s.contains("not a valid skill: '/path/to/skill' (missing SKILL.md)"));

        let err = SkillcError::EmptyQuery;
        let s = err.to_string();
        assert!(s.contains("error[E004]"));
        assert!(s.contains("empty query"));

        let err = SkillcError::SectionNotFound("Quick Start".to_string());
        let s = err.to_string();
        assert!(s.contains("error[E020]"));
        assert!(s.contains("section not found: 'Quick Start'"));

        let err = SkillcError::FileNotFound("README.md".to_string());
        let s = err.to_string();
        assert!(s.contains("error[E021]"));
        assert!(s.contains("file not found: 'README.md'"));

        let err = SkillcError::NoLocalLogs;
        let s = err.to_string();
        assert!(s.contains("error[E040]"));
        assert!(s.contains("no local logs found"));
    }

    #[test]
    fn test_skillc_error_codes() {
        assert_eq!(
            SkillcError::SkillNotFound("x".into()).code(),
            ErrorCode::E001
        );
        assert_eq!(
            SkillcError::IndexUnusable("x".into()).code(),
            ErrorCode::E002
        );
        assert_eq!(
            SkillcError::IndexHashCollision("x".into()).code(),
            ErrorCode::E003
        );
        assert_eq!(SkillcError::EmptyQuery.code(), ErrorCode::E004);
        assert_eq!(
            SkillcError::NotAValidSkill("x".into()).code(),
            ErrorCode::E010
        );
        assert_eq!(
            SkillcError::MissingFrontmatterField("x".into()).code(),
            ErrorCode::E011
        );
        assert_eq!(
            SkillcError::PathEscapesRoot("x".into()).code(),
            ErrorCode::E012
        );
        assert_eq!(
            SkillcError::SectionNotFound("x".into()).code(),
            ErrorCode::E020
        );
        assert_eq!(
            SkillcError::FileNotFound("x".into()).code(),
            ErrorCode::E021
        );
        assert_eq!(
            SkillcError::DirectoryNotFound("x".into()).code(),
            ErrorCode::E022
        );
        assert_eq!(
            SkillcError::InvalidQueryType("x".into()).code(),
            ErrorCode::E030
        );
        assert_eq!(
            SkillcError::InvalidFilter("x".into()).code(),
            ErrorCode::E031
        );
        assert_eq!(SkillcError::NoLocalLogs.code(), ErrorCode::E040);
        assert_eq!(
            SkillcError::SkillAlreadyExists("x".into()).code(),
            ErrorCode::E050
        );
        assert_eq!(
            SkillcError::InvalidOption("x".into()).code(),
            ErrorCode::E100
        );
        assert_eq!(SkillcError::Internal("x".into()).code(), ErrorCode::E999);
    }

    #[test]
    fn test_skillc_warning_display() {
        // Tests check for content presence since output includes ANSI color codes
        let warn = SkillcWarning::MultipleMatches("section".to_string());
        let s = warn.to_string();
        assert!(s.contains("warning[W001]"));
        assert!(s.contains("multiple matches for 'section'; showing first"));

        let warn = SkillcWarning::LoggingDisabled;
        let s = warn.to_string();
        assert!(s.contains("warning[W002]"));
        assert!(s.contains("logging disabled; run 'skc sync' after session to merge logs"));

        let warn = SkillcWarning::StaleLogs("rust".to_string());
        let s = warn.to_string();
        assert!(s.contains("warning[W003]"));
        assert!(s.contains("stale local logs for 'rust'; run 'skc sync' to upload"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err: SkillcError = io_err.into();
        assert_eq!(err.code(), ErrorCode::E999);
        assert!(err.to_string().contains("file not found"));
    }

    #[test]
    fn test_error_source() {
        use std::error::Error;

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err = SkillcError::Io(io_err);
        assert!(err.source().is_some());

        let err = SkillcError::SkillNotFound("x".to_string());
        assert!(err.source().is_none());
    }
}
