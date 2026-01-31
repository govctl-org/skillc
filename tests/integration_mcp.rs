//! Integration tests for MCP server per RFC-0007:C-MCP-SERVER.
//!
//! Tests the MCP server via subprocess stdio communication.

mod common;

use common::TestContext;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// MCP client for testing
struct McpTestClient {
    child: Child,
    reader: BufReader<std::process::ChildStdout>,
    next_id: u64,
}

impl McpTestClient {
    /// Spawn the MCP server process
    fn spawn() -> Self {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin!("skc"))
            .args(["mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn skc mcp");

        let stdout = child.stdout.take().expect("stdout not available");
        let reader = BufReader::new(stdout);

        Self {
            child,
            reader,
            next_id: 1,
        }
    }

    /// Spawn with TestContext for full isolation (SKILLC_HOME + working directory).
    ///
    /// Per [[RFC-0009:C-ENV-OVERRIDE]], uses `SKILLC_HOME` for cross-platform isolation.
    fn spawn_with_context(ctx: &TestContext) -> Self {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin!("skc"))
            .args(["mcp"])
            .current_dir(ctx.project_dir())
            .env("SKILLC_HOME", ctx.mock_home())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn skc mcp");

        let stdout = child.stdout.take().expect("stdout not available");
        let reader = BufReader::new(stdout);

        Self {
            child,
            reader,
            next_id: 1,
        }
    }

    /// Send a JSON-RPC request and get the response
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        let stdin = self.child.stdin.as_mut().expect("stdin not available");
        let request_str = serde_json::to_string(&request).expect("serialize request");
        writeln!(stdin, "{}", request_str).expect("write to stdin");
        stdin.flush().expect("flush stdin");

        // Read response
        let mut line = String::new();
        self.reader.read_line(&mut line).expect("read response");

        serde_json::from_str(&line).expect("parse response JSON")
    }

    /// Send a JSON-RPC notification (no response expected)
    fn notify(&mut self, method: &str, params: Value) {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        let stdin = self.child.stdin.as_mut().expect("stdin not available");
        let notification_str =
            serde_json::to_string(&notification).expect("serialize notification");
        writeln!(stdin, "{}", notification_str).expect("write to stdin");
        stdin.flush().expect("flush stdin");
    }

    /// Send initialization request and notification (required by MCP protocol)
    fn initialize(&mut self) -> Value {
        let response = self.request(
            "initialize",
            json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "test-client",
                    "version": "0.1.0"
                }
            }),
        );

        // Must send initialized notification after initialize request
        self.notify("notifications/initialized", json!({}));

        response
    }

    /// Call a tool
    fn call_tool(&mut self, name: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({
                "name": name,
                "arguments": arguments
            }),
        )
    }

    /// List available tools
    fn list_tools(&mut self) -> Value {
        self.request("tools/list", json!({}))
    }
}

impl Drop for McpTestClient {
    fn drop(&mut self) {
        // Try graceful shutdown first
        if let Some(stdin) = self.child.stdin.take() {
            drop(stdin); // Close stdin to signal EOF
        }

        // Give it a moment to shut down gracefully
        std::thread::sleep(Duration::from_millis(100));

        // Force kill if still running
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Test MCP server initialization
#[test]
fn test_mcp_initialize() {
    let mut client = McpTestClient::spawn();
    let response = client.initialize();

    // Should have result with server info
    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");

    // Check server capabilities
    assert!(
        result.get("capabilities").is_some(),
        "should have capabilities"
    );
    assert!(result.get("serverInfo").is_some(), "should have serverInfo");
}

/// Test listing available tools
#[test]
fn test_mcp_list_tools() {
    let mut client = McpTestClient::spawn();
    client.initialize();

    let response = client.list_tools();
    assert!(response.get("result").is_some(), "should have result");

    let result = response.get("result").expect("should have result");
    let tools = result.get("tools").expect("should have tools array");
    let tools_array = tools.as_array().expect("tools should be array");

    // Should have all expected tools
    let tool_names: Vec<&str> = tools_array
        .iter()
        .filter_map(|t| t.get("name").and_then(|n| n.as_str()))
        .collect();

    assert!(
        tool_names.contains(&"skc_outline"),
        "should have skc_outline"
    );
    assert!(tool_names.contains(&"skc_show"), "should have skc_show");
    assert!(tool_names.contains(&"skc_open"), "should have skc_open");
    assert!(
        tool_names.contains(&"skc_sources"),
        "should have skc_sources"
    );
    assert!(tool_names.contains(&"skc_search"), "should have skc_search");
    assert!(tool_names.contains(&"skc_stats"), "should have skc_stats");
    assert!(tool_names.contains(&"skc_build"), "should have skc_build");
    assert!(tool_names.contains(&"skc_init"), "should have skc_init");
    assert!(tool_names.contains(&"skc_lint"), "should have skc_lint");
}

/// Test skc_outline tool
#[test]
fn test_mcp_outline_tool() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("mcp-test-skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_outline",
        json!({
            "skill": "mcp-test-skill"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");

    // Content should be JSON with headings
    let text = content_array[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    let headings: Value = serde_json::from_str(text).expect("should parse as JSON");
    assert!(headings.is_array(), "should be array of headings");
}

/// Test skc_show tool
#[test]
fn test_mcp_show_tool() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("show-skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_show",
        json!({
            "skill": "show-skill",
            "section": "show-skill"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");
}

/// Test skc_show tool with non-existent section
#[test]
fn test_mcp_show_tool_not_found() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("show-skill-2");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_show",
        json!({
            "skill": "show-skill-2",
            "section": "nonexistent-section-xyz"
        }),
    );

    // Should return error in result
    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");

    // Check isError flag
    let is_error = result.get("isError").and_then(|v| v.as_bool());
    assert_eq!(is_error, Some(true), "should be an error");
}

/// Test skc_sources tool
#[test]
fn test_mcp_sources_tool() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("sources-skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_sources",
        json!({
            "skill": "sources-skill"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");

    // Content should be JSON with file entries
    let text = content_array[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    let sources: Value = serde_json::from_str(text).expect("should parse as JSON");
    assert!(sources.is_array(), "should be array of sources");
}

/// Test skc_lint tool
#[test]
fn test_mcp_lint_tool() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("lint-skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_lint",
        json!({
            "skill": "lint-skill"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");

    // Content should be JSON with lint result
    let text = content_array[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    let lint_result: Value = serde_json::from_str(text).expect("should parse as JSON");
    assert!(lint_result.get("skill").is_some(), "should have skill");
    assert!(
        lint_result.get("diagnostics").is_some(),
        "should have diagnostics"
    );
}

/// Test skc_init tool
#[test]
fn test_mcp_init_tool() {
    let ctx = TestContext::new().with_project();
    ctx.ensure_global_skills_dir();

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_init",
        json!({
            "name": "mcp-created-skill",
            "global": true
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");

    // Content should be JSON with success
    let text = content_array[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    let init_result: Value = serde_json::from_str(text).expect("should parse as JSON");
    assert_eq!(
        init_result.get("success").and_then(|v| v.as_bool()),
        Some(true),
        "should succeed"
    );

    // Verify skill was created in mock home's global source store
    let expected_path = ctx
        .mock_home()
        .join(".skillc")
        .join("skills")
        .join("mcp-created-skill")
        .join("SKILL.md");
    assert!(
        expected_path.exists(),
        "skill should be created at {}",
        expected_path.display()
    );
}

/// Test error handling for non-existent skill
#[test]
fn test_mcp_tool_skill_not_found() {
    // Use TestContext to isolate from real global store
    let ctx = TestContext::new().with_project();
    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_outline",
        json!({
            "skill": "nonexistent-skill-xyz-12345"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");

    // Should be an error
    let is_error = result.get("isError").and_then(|v| v.as_bool());
    assert_eq!(is_error, Some(true), "should be an error");

    // Error message should contain E001
    let content = result.get("content").expect("should have content");
    let text = content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    assert!(text.contains("E001"), "should have E001 error code");
}

/// Test skc_open tool
#[test]
fn test_mcp_open_tool() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("open-skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_open",
        json!({
            "skill": "open-skill",
            "path": "SKILL.md"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");

    // Content should contain the skill file contents
    let text = content_array[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    assert!(text.contains("open-skill"), "should contain skill name");
}

/// Test skc_open tool with max_lines
#[test]
fn test_mcp_open_tool_with_max_lines() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("lines-skill");

    // Add more lines to the SKILL.md
    let skill_md = ctx
        .project_dir()
        .join(".skillc")
        .join("skills")
        .join("lines-skill")
        .join("SKILL.md");
    let content = (1..=20)
        .map(|i| format!("Line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(
        &skill_md,
        format!(
            "---\nname: lines-skill\ndescription: test\n---\n# lines-skill\n{}",
            content
        ),
    )
    .expect("write skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_open",
        json!({
            "skill": "lines-skill",
            "path": "SKILL.md",
            "max_lines": 5
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let text = content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");

    // Should be truncated
    assert!(
        text.contains("... ("),
        "should indicate truncation: {}",
        text
    );
}

/// Test skc_outline tool with level filter
#[test]
fn test_mcp_outline_tool_with_level() {
    let ctx = TestContext::new().with_project();

    // Create skill with multiple heading levels
    ctx.create_skill_with_content(
        "level-skill",
        "---\nname: level-skill\ndescription: test\n---\n# H1\n## H2\n### H3\n#### H4",
    );

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_outline",
        json!({
            "skill": "level-skill",
            "level": 2
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let text = content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");

    let headings: Value = serde_json::from_str(text).expect("parse JSON");
    let headings_arr = headings.as_array().expect("should be array");

    // Should only have level 1 and 2 headings
    for h in headings_arr {
        let level = h
            .get("level")
            .and_then(|l| l.as_u64())
            .expect("should have level");
        assert!(level <= 2, "level should be <= 2, got {}", level);
    }
}

/// Test skc_show tool with max_lines
#[test]
fn test_mcp_show_tool_with_max_lines() {
    let ctx = TestContext::new().with_project();

    // Create skill with many lines
    let lines = (1..=30)
        .map(|i| format!("Content line {}", i))
        .collect::<Vec<_>>()
        .join("\n");
    ctx.create_skill_with_content(
        "show-lines-skill",
        &format!(
            "---\nname: show-lines-skill\ndescription: test\n---\n# Main Section\n{}",
            lines
        ),
    );

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_show",
        json!({
            "skill": "show-lines-skill",
            "section": "Main Section",
            "max_lines": 5
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let text = content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");

    // Should be truncated
    assert!(
        text.contains("... ("),
        "should indicate truncation: {}",
        text
    );
}

/// Test skc_stats tool
#[test]
fn test_mcp_stats_tool() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("stats-skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_stats",
        json!({
            "skill": "stats-skill"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");

    // Should be valid JSON
    let text = content_array[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    let _: Value = serde_json::from_str(text).expect("should parse as JSON");
}

/// Test skc_stats tool with group_by
#[test]
fn test_mcp_stats_tool_group_by() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("stats-group-skill");

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    for group_by in &[
        "files", "sections", "commands", "projects", "errors", "search",
    ] {
        let response = client.call_tool(
            "skc_stats",
            json!({
                "skill": "stats-group-skill",
                "group_by": group_by
            }),
        );

        assert!(
            response.get("result").is_some(),
            "should have result for group_by={}",
            group_by
        );
    }
}

/// Test skc_list tool
#[test]
fn test_mcp_list_tool() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("list-mcp-skill");
    ctx.ensure_global_skills_dir();

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    // Run from project directory context
    let response = client.call_tool("skc_list", json!({}));

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let content_array = content.as_array().expect("content should be array");
    assert!(!content_array.is_empty(), "should have content items");

    // Should be valid JSON with skills array
    let text = content_array[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    let parsed: Value = serde_json::from_str(text).expect("should parse as JSON");
    assert!(parsed.get("skills").is_some(), "should have skills array");
}

/// Test skc_list tool with filters
#[test]
fn test_mcp_list_tool_with_filters() {
    let ctx = TestContext::new().with_project();
    ctx.create_skill("filter-skill-1");
    ctx.create_skill("filter-skill-2");
    ctx.ensure_global_skills_dir();

    let mut client = McpTestClient::spawn_with_context(&ctx);
    client.initialize();

    let response = client.call_tool(
        "skc_list",
        json!({
            "scope": "project",
            "status": "not-built",
            "limit": 1,
            "pattern": "filter-*"
        }),
    );

    assert!(response.get("result").is_some(), "should have result");
    let result = response.get("result").expect("should have result");
    let content = result.get("content").expect("should have content");
    let text = content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .expect("should have text");
    let parsed: Value = serde_json::from_str(text).expect("should parse as JSON");

    // Should have at most 1 skill due to limit
    let skills = parsed
        .get("skills")
        .and_then(|s| s.as_array())
        .expect("skills array");
    assert!(skills.len() <= 1, "should respect limit");
}
