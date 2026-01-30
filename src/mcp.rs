//! MCP server implementation per [[RFC-0007:C-MCP-SERVER]] and [[RFC-0007:C-COMMANDS]]
//!
//! Provides structured agent interface via Model Context Protocol.
//! Uses the official Rust SDK from <https://github.com/modelcontextprotocol/rust-sdk>

use crate::config::{get_target_path, global_source_store};
use crate::{InitOptions, LintOptions, OutputFormat, QueryType, StatsOptions};
use rmcp::ErrorData as McpError;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, Content, ServerCapabilities, ServerInfo};
use rmcp::transport::stdio;
use rmcp::{ServerHandler, ServiceExt, tool, tool_handler, tool_router};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Use core Result to avoid conflict with crate's Result type alias
type McpResult<T> = core::result::Result<T, McpError>;

/// Convert SkillcError to McpError for use in MCP handlers
fn to_mcp_err(e: crate::SkillcError) -> McpError {
    McpError::internal_error(e.to_string(), None)
}

/// Parameters for skc_outline tool per [[RFC-0002:C-OUTLINE]]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct OutlineParams {
    /// Name of the skill to outline
    pub skill: String,
    /// Maximum heading level to include (1-6, optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<usize>,
}

/// Parameters for skc_show tool per [[RFC-0002:C-SHOW]]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ShowParams {
    /// Name of the skill
    pub skill: String,
    /// Section to retrieve
    pub section: String,
    /// Limit search to specific file (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Maximum lines to return (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<usize>,
}

/// Parameters for skc_open tool per [[RFC-0002:C-OPEN]]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct OpenParams {
    /// Name of the skill
    pub skill: String,
    /// Path to the file within the skill
    pub path: String,
    /// Maximum lines to return (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_lines: Option<usize>,
}

/// Parameters for skc_sources tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourcesParams {
    /// Name of the skill
    pub skill: String,
    /// Maximum tree depth (optional, default: unlimited)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// Scope to subdirectory (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dir: Option<String>,
    /// Maximum entries (optional, default: 100)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Glob pattern filter (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// Parameters for skc_search tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Name of the skill
    pub skill: String,
    /// Search query
    pub query: String,
    /// Maximum results (optional, default: 10)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// Parameters for skc_stats tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct StatsParams {
    /// Name of the skill
    pub skill: String,
    /// Group by dimension: files, sections, commands, projects, errors (default: summary/none)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<String>,
    /// Include accesses on or after (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Include accesses on or before (ISO 8601)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    /// Filter by project directory
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<Vec<String>>,
}

/// Parameters for skc_build tool
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct BuildParams {
    /// Name of the skill to build
    pub skill: String,
    /// Target platform (optional, default: claude)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// Parameters for skc_init tool per [[RFC-0007:C-COMMANDS]]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct InitParams {
    /// Skill name to create (omit for project initialization only)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Create in global source store instead of project-local (default: false)
    #[serde(default)]
    pub global: bool,
}

/// Parameters for skc_lint tool per [[RFC-0007:C-COMMANDS]]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct LintParams {
    /// Name of the skill to lint
    pub skill: String,
    /// Force linting even if skill is compiled (default: false)
    #[serde(default)]
    pub force: bool,
}

/// Parameters for skc_list tool per [[RFC-0007:C-LIST]]
#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ListParams {
    /// Filter by scope: "project", "global", or omit for all
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    /// Filter by status: "normal", "not-built", "stale", or omit for all
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Maximum skills to return
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Filter by skill name (glob pattern)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Enable obsolete runtime detection (default: false)
    #[serde(default)]
    pub check_obsolete: bool,
}

/// skillc MCP Server per [[RFC-0007:C-MCP-SERVER]]
#[derive(Clone)]
pub struct SkillcServer {
    tool_router: ToolRouter<Self>,
}

impl SkillcServer {
    pub fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }
}

impl Default for SkillcServer {
    fn default() -> Self {
        Self::new()
    }
}

#[tool_router]
impl SkillcServer {
    /// List all sections in a skill
    #[tool(
        description = "List all sections in a skill. Returns JSON array of {file, level, heading}. Use 'level' param to filter by max heading level (1-6).",
        annotations(read_only_hint = true)
    )]
    async fn skc_outline(&self, params: Parameters<OutlineParams>) -> McpResult<CallToolResult> {
        match crate::outline(&params.0.skill, params.0.level, OutputFormat::Json) {
            Ok(json) => Ok(CallToolResult::success(vec![Content::text(json)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// Retrieve section content from a skill
    #[tool(
        description = "Retrieve markdown section content by heading. Returns raw text. Use 'max_lines' to limit output.",
        annotations(read_only_hint = true)
    )]
    async fn skc_show(&self, params: Parameters<ShowParams>) -> McpResult<CallToolResult> {
        match crate::show(
            &params.0.skill,
            &params.0.section,
            params.0.file.as_deref(),
            params.0.max_lines,
            OutputFormat::Text,
        ) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// Retrieve file content from a skill
    #[tool(
        description = "Retrieve raw file content by path. Returns raw text. Use 'max_lines' to limit output.",
        annotations(read_only_hint = true)
    )]
    async fn skc_open(&self, params: Parameters<OpenParams>) -> McpResult<CallToolResult> {
        match crate::open(
            &params.0.skill,
            &params.0.path,
            params.0.max_lines,
            OutputFormat::Text,
        ) {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// List source files in a skill
    #[tool(
        description = "List source files as tree structure. Returns JSON array of {path, type, children?}.",
        annotations(read_only_hint = true)
    )]
    async fn skc_sources(&self, params: Parameters<SourcesParams>) -> McpResult<CallToolResult> {
        match crate::sources(
            &params.0.skill,
            params.0.depth,
            params.0.dir.as_deref(),
            params.0.limit.unwrap_or(100),
            params.0.pattern.as_deref(),
            OutputFormat::Json,
        ) {
            Ok(json) => Ok(CallToolResult::success(vec![Content::text(json)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// Search across skill content
    #[tool(
        description = "Full-text search in skill content. Returns JSON array of {file, line, content, score}.",
        annotations(read_only_hint = true)
    )]
    async fn skc_search(&self, params: Parameters<SearchParams>) -> McpResult<CallToolResult> {
        match crate::search(
            &params.0.skill,
            &params.0.query,
            params.0.limit.unwrap_or(10),
            OutputFormat::Json,
        ) {
            Ok(json) => Ok(CallToolResult::success(vec![Content::text(json)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// Usage analytics for a skill
    #[tool(
        description = "Usage analytics for a skill. Returns JSON with access counts, popular sections, etc. Use group_by: summary, files, sections, commands, projects, errors, or search.",
        annotations(read_only_hint = true)
    )]
    async fn skc_stats(&self, params: Parameters<StatsParams>) -> McpResult<CallToolResult> {
        let query_type = match params.0.group_by.as_deref() {
            Some("files") => QueryType::Files,
            Some("sections") => QueryType::Sections,
            Some("commands") => QueryType::Commands,
            Some("projects") => QueryType::Projects,
            Some("errors") => QueryType::Errors,
            Some("search") => QueryType::Search,
            _ => QueryType::Summary, // None or "summary" → aggregate totals
        };
        match crate::stats(
            &params.0.skill,
            StatsOptions {
                query: query_type,
                format: OutputFormat::Json,
                since: params.0.since.clone(),
                until: params.0.until.clone(),
                projects: params.0.project.clone().unwrap_or_default(),
            },
        ) {
            Ok(json) => Ok(CallToolResult::success(vec![Content::text(json)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// Compile a skill
    #[tool(
        description = "Compile skill to target platform (claude, cursor). Returns {success, output_path}."
    )]
    async fn skc_build(&self, params: Parameters<BuildParams>) -> McpResult<CallToolResult> {
        // Resolve source
        let source = {
            let path = PathBuf::from(&params.0.skill);
            if path.exists() && path.is_dir() {
                path
            } else {
                global_source_store()
                    .map_err(to_mcp_err)?
                    .join(&params.0.skill)
            }
        };

        // Resolve runtime (use target param or default to claude)
        let target = params.0.target.as_deref().unwrap_or("claude");
        let skill_name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&params.0.skill);
        let runtime = get_target_path(target)
            .map_err(to_mcp_err)?
            .join(skill_name);

        match crate::compile(&source, &runtime) {
            Ok(()) => {
                let result = serde_json::json!({
                    "success": true,
                    "output_path": runtime.to_string_lossy()
                });
                Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// Initialize project or create a new skill per [[RFC-0006:C-INIT]]
    #[tool(
        description = "Initialize skillc project or create a new skill. Without name: creates .skillc/ structure. With name: creates skill template. With name+global: creates in global store."
    )]
    async fn skc_init(&self, params: Parameters<InitParams>) -> McpResult<CallToolResult> {
        let options = InitOptions {
            name: params.0.name.clone(),
            global: params.0.global,
        };

        match crate::init(options) {
            Ok(message) => {
                let result = serde_json::json!({
                    "success": true,
                    "message": message
                });
                Ok(CallToolResult::success(vec![Content::text(
                    result.to_string(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// List all skillc-managed skills per [[RFC-0007:C-LIST]]
    #[tool(
        description = "List all skillc-managed skills from source stores. Returns JSON with skills array containing name, scope, status, and paths.",
        annotations(read_only_hint = true)
    )]
    async fn skc_list(&self, params: Parameters<ListParams>) -> McpResult<CallToolResult> {
        use crate::list::{ListOptions, SkillScope, SkillStatus};

        // Convert string filters to enum types
        let scope = params.0.scope.as_deref().and_then(|s| match s {
            "project" => Some(SkillScope::Project),
            "global" => Some(SkillScope::Global),
            _ => None,
        });

        let status = params.0.status.as_deref().and_then(|s| match s {
            "normal" => Some(SkillStatus::Normal),
            "not-built" => Some(SkillStatus::NotBuilt),
            "obsolete" => Some(SkillStatus::Obsolete),
            _ => None,
        });

        let options = ListOptions {
            scope,
            status,
            limit: params.0.limit,
            pattern: params.0.pattern.clone(),
            check_obsolete: params.0.check_obsolete,
        };

        match crate::list::list(&options) {
            Ok(result) => {
                let json = serde_json::to_string(&result)
                    .unwrap_or_else(|e| format!(r#"{{"error": "serialization failed: {}"}}"#, e));
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }

    /// Lint a skill per [[RFC-0008]]
    #[tool(
        description = "Validate skill authoring quality. Returns diagnostics with rule IDs, severities, and messages.",
        annotations(read_only_hint = true)
    )]
    async fn skc_lint(&self, params: Parameters<LintParams>) -> McpResult<CallToolResult> {
        // Resolve skill path
        let skill_path = {
            let path = PathBuf::from(&params.0.skill);
            if path.exists() && path.is_dir() {
                path
            } else {
                global_source_store()
                    .map_err(to_mcp_err)?
                    .join(&params.0.skill)
            }
        };

        let options = LintOptions {
            force: params.0.force,
        };

        match crate::lint(&skill_path, options) {
            Ok(result) => {
                let json = serde_json::json!({
                    "skill": result.skill,
                    "path": result.path.to_string_lossy(),
                    "error_count": result.error_count,
                    "warning_count": result.warning_count,
                    "diagnostics": result.diagnostics
                });
                Ok(CallToolResult::success(vec![Content::text(
                    json.to_string(),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "error: {}",
                e
            ))])),
        }
    }
}

// Implement the server handler
#[tool_handler]
impl ServerHandler for SkillcServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "skillc - A development kit for Agent Skills. Use MCP tools to access skill content.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

/// Run the MCP server per [[RFC-0007:C-MCP-SERVER]]
pub async fn run_server() -> crate::error::Result<()> {
    let server = SkillcServer::new();

    // Start the server with stdio transport
    let service = server
        .serve(stdio())
        .await
        .map_err(|e| crate::error::SkillcError::Internal(format!("MCP server error: {}", e)))?;

    // Wait for shutdown
    service
        .waiting()
        .await
        .map_err(|e| crate::error::SkillcError::Internal(format!("MCP server error: {}", e)))?;

    Ok(())
}
