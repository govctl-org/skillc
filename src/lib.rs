//! skillc - A development kit for Agent Skills
//!
//! See [[RFC-0000]] for vision, [[RFC-0001]] for compilation spec, [[RFC-0002]] for gateway protocol,
//! [[RFC-0004]] for search protocol.

use clap::ValueEnum;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Output format for CLI and MCP commands
#[derive(Clone, Debug, ValueEnum, Default)]
pub enum OutputFormat {
    #[default]
    Text,
    Json,
}

/// Heading extracted from markdown files
#[derive(Debug)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub file: PathBuf,
    pub line_number: usize,
}

pub mod analytics;
pub mod compiler;
pub mod config;
pub mod deploy;
pub mod error;
pub mod frontmatter;
pub mod gateway;
pub mod index;
pub mod init;
pub mod lint;
pub mod list;
pub mod logging;
pub mod markdown;
pub mod mcp;
pub mod resolver;
pub mod search;
pub mod sync;
pub mod util;

pub use analytics::{QueryType, StatsOptions, stats};
pub use compiler::compile;
pub use error::{Result, SkillcError, SkillcWarning};
pub use gateway::{open, outline, show, sources};
pub use init::{InitOptions, init};
pub use lint::{Diagnostic, LintOptions, LintResult, Severity, lint};
pub use list::{ListOptions, ListResult, SkillScope, SkillStatus, format_list, list};
pub use resolver::{ResolvedSkill, resolve_skill};
pub use search::search;
pub use sync::{SyncOptions, sync};

// Global verbose flag
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Enable or disable verbose output mode.
pub fn set_verbose(enabled: bool) {
    VERBOSE.store(enabled, Ordering::SeqCst);
}

/// Check if verbose output is enabled.
pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::SeqCst)
}

/// Print a verbose message to stderr if verbose mode is enabled.
#[macro_export]
macro_rules! verbose {
    ($($arg:tt)*) => {
        if $crate::is_verbose() {
            eprintln!("[verbose] {}", format!($($arg)*));
        }
    };
}
