//! Configuration and path helpers per [[RFC-0009]] and [[ADR-0001]]/[[ADR-0002]]

use crate::error::{Result, SkillcError};
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use strum::EnumProperty;

/// Tokenizer preference for search indexing per [[RFC-0009:C-TOKENIZER]]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tokenizer {
    /// Split on whitespace and punctuation (default)
    #[default]
    Ascii,
    /// Character-level tokenization for CJK content
    Cjk,
}

impl FromStr for Tokenizer {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "ascii" => Ok(Tokenizer::Ascii),
            "cjk" => Ok(Tokenizer::Cjk),
            _ => Err(()),
        }
    }
}

impl Tokenizer {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Tokenizer::Ascii => "ascii",
            Tokenizer::Cjk => "cjk",
        }
    }
}

/// Deployment target for agent skills per ADR-0002.
///
/// Each variant maps to a specific directory structure in the user's home.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    clap::ValueEnum,
    strum::Display,
    strum::EnumString,
    strum::EnumIter,
    strum::EnumProperty,
)]
#[strum(serialize_all = "lowercase")]
#[clap(rename_all = "lowercase")]
pub enum Target {
    /// Claude Code → ~/.claude/skills/
    #[strum(props(dir = ".claude"))]
    Claude,
    /// Codex → ~/.codex/skills/
    #[strum(props(dir = ".codex"))]
    Codex,
    /// GitHub Copilot → ~/.github/skills/
    #[strum(props(dir = ".github"))]
    Copilot,
    /// Cursor → ~/.cursor/skills/
    #[strum(props(dir = ".cursor"))]
    Cursor,
    /// Gemini → ~/.gemini/skills/
    #[strum(props(dir = ".gemini"))]
    Gemini,
    /// Kiro → ~/.kiro/skills/
    #[strum(props(dir = ".kiro"))]
    Kiro,
    /// OpenCode → ~/.opencode/skills/
    #[strum(props(dir = ".opencode"))]
    Opencode,
    /// Trae → ~/.trae/skills/
    #[strum(props(dir = ".trae"))]
    Trae,
}

impl Target {
    /// Get the directory name for this target (e.g., ".claude", ".github").
    pub fn dir_name(&self) -> &'static str {
        self.get_str("dir").expect("all variants have dir prop")
    }

    /// Get the global skills path for this target.
    pub fn global_path(&self) -> Result<PathBuf> {
        let home = dirs::home_dir().ok_or_else(|| {
            SkillcError::Internal("could not determine home directory".to_string())
        })?;
        Ok(home.join(self.dir_name()).join("skills"))
    }

    /// Get the project-local skills path for this target.
    pub fn project_path(&self, project_root: &Path) -> PathBuf {
        project_root.join(self.dir_name()).join("skills")
    }
}

/// Target specification for CLI: either a known target or a custom path.
///
/// This allows `--target claude` (known) or `--target /custom/path` (custom).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetSpec {
    /// A known agent target
    Known(Target),
    /// A custom path (for testing or advanced users)
    Custom(PathBuf),
}

impl TargetSpec {
    /// Get the skills directory path for this target.
    ///
    /// For known targets, resolves to global or project-local path.
    /// For custom paths, returns the path directly.
    pub fn skills_path(&self, project_root: Option<&Path>) -> Result<PathBuf> {
        match self {
            TargetSpec::Known(t) => match project_root {
                Some(root) => Ok(t.project_path(root)),
                None => t.global_path(),
            },
            TargetSpec::Custom(p) => Ok(p.clone()),
        }
    }

    /// Check if this is a known target (eligible for project-local deployment).
    pub fn is_known(&self) -> bool {
        matches!(self, TargetSpec::Known(_))
    }
}

impl std::fmt::Display for TargetSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TargetSpec::Known(t) => write!(f, "{}", t),
            TargetSpec::Custom(p) => write!(f, "{}", p.display()),
        }
    }
}

impl FromStr for TargetSpec {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        // Try known target first, fall back to custom path
        match s.parse::<Target>() {
            Ok(t) => Ok(TargetSpec::Known(t)),
            Err(_) => Ok(TargetSpec::Custom(PathBuf::from(s))),
        }
    }
}

/// Search configuration section per [[RFC-0009:C-FILES]]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchConfig {
    /// Tokenizer for search indexing
    #[serde(default)]
    pub tokenizer: Option<Tokenizer>,
}

/// Configuration file schema per [[RFC-0009:C-FILES]]
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillcConfig {
    /// Schema version (optional, default: 1)
    #[serde(default)]
    pub version: Option<u32>,

    /// Search settings
    #[serde(default)]
    pub search: SearchConfig,
}

/// Load and parse a config file, handling errors per [[RFC-0009:C-FILES]]
fn load_config_file(path: &Path) -> Option<SkillcConfig> {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return None,
    };

    // Parse TOML
    let config: SkillcConfig = match toml::from_str(&content) {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "warning: Failed to parse config file {}: {}",
                path.display(),
                e
            );
            return None;
        }
    };

    // Version validation per [[RFC-0009:C-FILES]]
    if let Some(version) = config.version {
        if version == 0 {
            eprintln!(
                "error: Invalid config version {} in {} (must be positive integer)",
                version,
                path.display()
            );
            return None; // Ignore entire file
        }
        if version > 1 {
            eprintln!(
                "warning: Config version {} in {} is newer than supported (1), using recognized fields only",
                version,
                path.display()
            );
            // Continue with recognized fields
        }
    }

    Some(config)
}

/// Find project config by walking up from cwd per [[RFC-0009:C-RESOLUTION]]
fn find_project_config() -> Option<PathBuf> {
    let mut dir = env::current_dir().ok()?;

    loop {
        let config_path = dir.join(".skillc").join("config.toml");
        if config_path.exists() {
            return Some(config_path);
        }

        if !dir.pop() {
            break;
        }
    }

    None
}

/// Get the resolved tokenizer preference per [[RFC-0009:C-RESOLUTION]]
///
/// Resolution order (highest priority first):
/// 1. SKILLC_TOKENIZER environment variable
/// 2. Project config
/// 3. Global config
/// 4. Default (ascii)
pub fn get_tokenizer() -> Tokenizer {
    // 1. Environment variable
    if let Ok(val) = env::var("SKILLC_TOKENIZER") {
        if let Ok(t) = val.parse::<Tokenizer>() {
            return t;
        } else {
            eprintln!(
                "warning: Invalid SKILLC_TOKENIZER value '{}', ignoring",
                val
            );
        }
    }

    // 2. Project config
    if let Some(path) = find_project_config()
        && let Some(config) = load_config_file(&path)
        && let Some(t) = config.search.tokenizer
    {
        return t;
    }

    // 3. Global config
    if let Ok(global_config_path) = global_skillc_dir().map(|d| d.join("config.toml"))
        && let Some(config) = load_config_file(&global_config_path)
        && let Some(t) = config.search.tokenizer
    {
        return t;
    }

    // 4. Default
    Tokenizer::default()
}

/// Get the global skillc directory.
///
/// Per [[RFC-0009:C-ENV-OVERRIDE]], checks `SKILLC_HOME` first, then falls back to `~/.skillc/`.
/// When `SKILLC_HOME` is set, it acts as the home directory override, so `.skillc` is appended.
///
/// Returns error if home directory cannot be determined.
pub fn global_skillc_dir() -> Result<PathBuf> {
    // Check SKILLC_HOME override first (enables cross-platform test isolation)
    // SKILLC_HOME acts as home directory override, so we append .skillc
    if let Ok(skillc_home) = env::var("SKILLC_HOME") {
        return Ok(PathBuf::from(skillc_home).join(".skillc"));
    }

    let home = dirs::home_dir()
        .ok_or_else(|| SkillcError::Internal("could not determine home directory".to_string()))?;
    Ok(home.join(".skillc"))
}

/// Get the global source store (~/.skillc/skills/).
pub fn global_source_store() -> Result<PathBuf> {
    Ok(global_skillc_dir()?.join("skills"))
}

/// Find project root by walking up from CWD, looking for `.skillc/` directory.
///
/// Returns the project root directory (containing `.skillc/`) if found.
/// Note: The home directory (or SKILLC_HOME) is excluded - its `.skillc/` is the global store, not a project.
pub fn find_project_root() -> Option<PathBuf> {
    // Get the home directory to exclude from project root search
    // If SKILLC_HOME is set, use that; otherwise use the real home
    let excluded_home = if let Ok(skillc_home) = env::var("SKILLC_HOME") {
        Some(PathBuf::from(skillc_home))
    } else {
        dirs::home_dir()
    };

    let mut dir = env::current_dir().ok()?;

    loop {
        // Don't treat home directory as project root (its .skillc is the global store)
        if Some(&dir) != excluded_home.as_ref() && dir.join(".skillc").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            break;
        }
    }

    None
}

/// Find a skill in the project source store by walking up from CWD.
///
/// Returns `(skill_path, project_root)` if found.
pub fn find_project_skill(skill_name: &str) -> Option<(PathBuf, PathBuf)> {
    let project_root = find_project_root()?;
    let skill_path = crate::util::project_skill_dir(&project_root, skill_name);

    if crate::util::is_valid_skill(&skill_path) {
        Some((skill_path, project_root))
    } else {
        None
    }
}

/// Resolve the source store and whether it's project-local.
///
/// Returns `(source_store_path, is_local)`.
pub fn resolve_source_store() -> Result<(PathBuf, bool)> {
    if let Some(root) = find_project_root() {
        Ok((crate::util::project_skills_dir(&root), true))
    } else {
        Ok((global_source_store()?, false))
    }
}

/// Get the target path from a string (for MCP/legacy compatibility).
///
/// Parses to `Target` enum if known, otherwise treats as direct path.
pub fn get_target_path(target: &str) -> Result<PathBuf> {
    match target.parse::<Target>() {
        Ok(t) => t.global_path(),
        Err(_) => Ok(PathBuf::from(target)),
    }
}

/// Get the global registry path (~/.skillc/registry.json)
pub fn global_registry_path() -> Result<PathBuf> {
    Ok(global_skillc_dir()?.join("registry.json"))
}

/// Get the project-local runtime SSOT store (`<project>/.skillc/runtime/`)
///
/// Walks up from CWD to find project root, then returns its runtime directory.
/// Returns None if no project root is found.
pub fn project_runtime_store() -> Option<PathBuf> {
    let project_root = find_project_root()?;
    Some(crate::util::project_runtime_dir(&project_root))
}

/// Get the global runtime SSOT store (~/.skillc/runtime/)
///
/// This is the SSOT location for globally compiled skills,
/// distinct from agent directories like ~/.claude/skills/.
pub fn global_runtime_store() -> Result<PathBuf> {
    Ok(global_skillc_dir()?.join("runtime"))
}

/// Ensure a directory exists, creating it if necessary
pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Get canonicalized current working directory as string, with fallback.
/// Per [[RFC-0007:C-LOGGING]], paths must be canonicalized (symlinks resolved).
pub fn get_cwd() -> String {
    env::current_dir()
        .and_then(|p| p.canonicalize())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "<unknown>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_tokenizer_from_str() {
        assert_eq!("ascii".parse::<Tokenizer>().ok(), Some(Tokenizer::Ascii));
        assert_eq!("ASCII".parse::<Tokenizer>().ok(), Some(Tokenizer::Ascii));
        assert_eq!("cjk".parse::<Tokenizer>().ok(), Some(Tokenizer::Cjk));
        assert_eq!("CJK".parse::<Tokenizer>().ok(), Some(Tokenizer::Cjk));
        assert_eq!("invalid".parse::<Tokenizer>().ok(), None);
        assert_eq!("".parse::<Tokenizer>().ok(), None);
    }

    #[test]
    fn test_tokenizer_as_str() {
        assert_eq!(Tokenizer::Ascii.as_str(), "ascii");
        assert_eq!(Tokenizer::Cjk.as_str(), "cjk");
    }

    #[test]
    fn test_tokenizer_default() {
        assert_eq!(Tokenizer::default(), Tokenizer::Ascii);
    }

    #[test]
    fn test_load_config_file_not_found() {
        let result = load_config_file(Path::new("/nonexistent/config.toml"));
        assert!(result.is_none());
    }

    #[test]
    fn test_load_config_file_empty() {
        let temp = TempDir::new().expect("create temp dir");
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "").expect("write config");

        let result = load_config_file(&config_path);
        assert!(result.is_some());
        let config = result.expect("expected result");
        assert_eq!(config.version, None);
        assert_eq!(config.search.tokenizer, None);
    }

    #[test]
    fn test_load_config_file_with_tokenizer() {
        let temp = TempDir::new().expect("create temp dir");
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
[search]
tokenizer = "cjk"
"#,
        )
        .expect("test operation");

        let result = load_config_file(&config_path);
        assert!(result.is_some());
        let config = result.expect("expected result");
        assert_eq!(config.search.tokenizer, Some(Tokenizer::Cjk));
    }

    #[test]
    fn test_load_config_file_with_version() {
        let temp = TempDir::new().expect("create temp dir");
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
version = 1

[search]
tokenizer = "ascii"
"#,
        )
        .expect("test operation");

        let result = load_config_file(&config_path);
        assert!(result.is_some());
        let config = result.expect("expected result");
        assert_eq!(config.version, Some(1));
        assert_eq!(config.search.tokenizer, Some(Tokenizer::Ascii));
    }

    #[test]
    fn test_load_config_file_invalid_version_zero() {
        let temp = TempDir::new().expect("create temp dir");
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
version = 0
"#,
        )
        .expect("test operation");

        // Version 0 is invalid - should ignore entire file
        let result = load_config_file(&config_path);
        assert!(result.is_none());
    }

    #[test]
    fn test_load_config_file_future_version() {
        let temp = TempDir::new().expect("create temp dir");
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
version = 99

[search]
tokenizer = "cjk"
"#,
        )
        .expect("test operation");

        // Future version - should warn but continue with recognized fields
        let result = load_config_file(&config_path);
        assert!(result.is_some());
        let config = result.expect("expected result");
        assert_eq!(config.search.tokenizer, Some(Tokenizer::Cjk));
    }

    #[test]
    fn test_load_config_file_unknown_keys() {
        let temp = TempDir::new().expect("create temp dir");
        let config_path = temp.path().join("config.toml");
        fs::write(
            &config_path,
            r#"
[search]
tokenizer = "ascii"
unknown_key = "ignored"

[unknown_section]
foo = "bar"
"#,
        )
        .expect("test operation");

        // Unknown keys should be ignored (serde default behavior)
        let result = load_config_file(&config_path);
        assert!(result.is_some());
        let config = result.expect("expected result");
        assert_eq!(config.search.tokenizer, Some(Tokenizer::Ascii));
    }

    #[test]
    fn test_load_config_file_invalid_toml() {
        let temp = TempDir::new().expect("create temp dir");
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "this is not valid toml [[[").expect("write invalid config");

        // Invalid TOML should return None
        let result = load_config_file(&config_path);
        assert!(result.is_none());
    }
}
