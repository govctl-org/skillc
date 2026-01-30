//! Integration tests for MCP server per RFC-0007:C-MCP-SERVER.
//!
//! Tests the MCP server via subprocess stdio communication.

mod common;

use common::create_minimal_skill;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use tempfile::TempDir;

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

    /// Spawn with custom environment
    fn spawn_with_env(env: &[(&str, &str)]) -> Self {
        let mut cmd = Command::new(assert_cmd::cargo::cargo_bin!("skc"));
        cmd.args(["mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for (key, value) in env {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().expect("failed to spawn skc mcp");
        let stdout = child.stdout.take().expect("stdout not available");
        let reader = BufReader::new(stdout);

        Self {
            child,
            reader,
            next_id: 1,
        }
    }

    /// Spawn with custom working directory
    fn spawn_in_dir(dir: &std::path::Path) -> Self {
        let mut child = Command::new(assert_cmd::cargo::cargo_bin!("skc"))
            .args(["mcp"])
            .current_dir(dir)
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
    let temp = TempDir::new().expect("create temp dir");
    let skill_name = "mcp-test-skill";
    common::create_project_skill(temp.path(), skill_name);

    // Run MCP server from project directory, use skill name (not path)
    let mut client = McpTestClient::spawn_in_dir(temp.path());
    client.initialize();

    let response = client.call_tool(
        "skc_outline",
        json!({
            "skill": skill_name
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
    let temp = TempDir::new().expect("create temp dir");
    create_minimal_skill(temp.path(), "show-skill");
    let skill_path = temp.path().join("show-skill");

    // Use direct path to skill, no env var needed
    let mut client = McpTestClient::spawn();
    client.initialize();

    let response = client.call_tool(
        "skc_show",
        json!({
            "skill": skill_path.to_str().expect("path to str"),
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
    let temp = TempDir::new().expect("create temp dir");
    create_minimal_skill(temp.path(), "show-skill-2");
    let skill_path = temp.path().join("show-skill-2");

    // Use direct path to skill, no env var needed
    let mut client = McpTestClient::spawn();
    client.initialize();

    let response = client.call_tool(
        "skc_show",
        json!({
            "skill": skill_path.to_str().expect("path to str"),
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
    let temp = TempDir::new().expect("create temp dir");
    let skill_name = "sources-skill";
    common::create_project_skill(temp.path(), skill_name);

    // Run MCP server from project directory, use skill name (not path)
    let mut client = McpTestClient::spawn_in_dir(temp.path());
    client.initialize();

    let response = client.call_tool(
        "skc_sources",
        json!({
            "skill": skill_name
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
    let temp = TempDir::new().expect("create temp dir");
    create_minimal_skill(temp.path(), "lint-skill");
    let skill_path = temp.path().join("lint-skill");

    // Use direct path to skill, no env var needed
    let mut client = McpTestClient::spawn();
    client.initialize();

    let response = client.call_tool(
        "skc_lint",
        json!({
            "skill": skill_path.to_str().expect("path to str")
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
#[cfg(unix)]
#[test]
fn test_mcp_init_tool() {
    let temp = TempDir::new().expect("create temp dir");

    // Use HOME env var override to redirect global source store
    let mock_home = common::create_mock_home(temp.path());

    let mut client =
        McpTestClient::spawn_with_env(&[("HOME", mock_home.to_str().expect("path to str"))]);
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
    let expected_path = mock_home
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
    let mut client = McpTestClient::spawn();
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
