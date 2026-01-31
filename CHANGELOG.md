# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.1] - 2026-01-31

### Added

- `SKILLC_HOME` env var overrides home directory detection (WI-2026-01-31-001)

### Fixed

- MCP lint/build use resolve_skill() (WI-2026-01-31-002)

## [0.1.0] - 2026-01-30

### Added

- RFC-0000 defines skillc vision aligned with Agent Skills ecosystem (WI-2026-01-25-001)
- RFC-0000 includes normative compatibility guarantees (format, offline, portability) (WI-2026-01-25-001)
- RFC-0001 specifies stub output format (WI-2026-01-25-002)
- RFC-0001 specifies manifest structure (WI-2026-01-25-002)
- RFC-0001 defines compilation constraints (WI-2026-01-25-002)
- Compiler accepts valid Agent Skills directory (WI-2026-01-26-001)
- Compiler produces stub SKILL.md per [RFC-0001:C-STUB](docs/rfc/RFC-0001.md#rfc-0001c-stub) (WI-2026-01-26-001)
- Compiler produces manifest.json per [RFC-0001:C-MANIFEST](docs/rfc/RFC-0001.md#rfc-0001c-manifest) (WI-2026-01-26-001)
- `skc build` CLI command works (WI-2026-01-26-001)
- Implement `skc outline` command per [RFC-0002:C-OUTLINE](docs/rfc/RFC-0002.md#rfc-0002c-outline) (WI-2026-01-26-002)
- Implement `skc show` command per [RFC-0002:C-SHOW](docs/rfc/RFC-0002.md#rfc-0002c-show) (WI-2026-01-26-002)
- Implement `skc open` command per [RFC-0002:C-OPEN](docs/rfc/RFC-0002.md#rfc-0002c-open) (WI-2026-01-26-002)
- Implement access logging per [RFC-0007:C-LOGGING](docs/rfc/RFC-0007.md#rfc-0007c-logging) (WI-2026-01-26-002)
- Implement skill resolution per [RFC-0007:C-RESOLUTION](docs/rfc/RFC-0007.md#rfc-0007c-resolution) (WI-2026-01-26-002)
- Implement stats analytics queries per [RFC-0003](docs/rfc/RFC-0003.md) (WI-2026-01-26-003)
- Support filters and JSON output schema (WI-2026-01-26-003)
- CLI integration test harness with snapshots (WI-2026-01-27-001)
- Integration coverage for gateway and stats commands (WI-2026-01-27-001)
- Draft [RFC-0004](docs/rfc/RFC-0004.md) Search Protocol (WI-2026-01-28-001)
- Create [RFC-0005](docs/rfc/RFC-0005.md) Error Code Registry (WI-2026-01-28-002)
- Define canonical error code table (WI-2026-01-28-002)
- Migrate [RFC-0004](docs/rfc/RFC-0004.md) C-ERRORS to reference [RFC-0005](docs/rfc/RFC-0005.md) (WI-2026-01-28-002)
- Build centralized error code registry in `error.rs` (WI-2026-01-29-001)
- Migrate all commands to use canonical error messages per [RFC-0005:C-CODES](docs/rfc/RFC-0005.md#rfc-0005c-codes) (WI-2026-01-29-001)
- Implement `skc search` command with FTS5 index (WI-2026-01-29-003)
- Build search index during `skc build` per [RFC-0004:C-INDEX](docs/rfc/RFC-0004.md#rfc-0004c-index) (WI-2026-01-29-003)
- Support .md and .txt file formats per [RFC-0004:C-FORMATS](docs/rfc/RFC-0004.md#rfc-0004c-formats) (WI-2026-01-29-003)
- E999 added to [RFC-0005](docs/rfc/RFC-0005.md) for internal errors (WI-2026-01-29-007)
- SKC_VERBOSE/-v flag for debug output (WI-2026-01-29-008)
- CLI examples in --help output (WI-2026-01-29-008)
- CONTRIBUTING.md documentation (WI-2026-01-29-008)
- Pass verbose flag through to library functions (WI-2026-01-29-009)
- Verbose output shows resolved paths for skill resolution (WI-2026-01-29-009)
- Verbose output shows timing information for operations (WI-2026-01-29-009)
- `skc sources` command with tree-style output (WI-2026-01-29-011)
- `--depth`, `--dir`, `--limit`, `--pattern` options (WI-2026-01-29-011)
- Amend [RFC-0007:C-LOGGING](docs/rfc/RFC-0007.md#rfc-0007c-logging) with fallback logging for sandboxed environments (WI-2026-01-29-012)
- Add [RFC-0007:C-SYNC](docs/rfc/RFC-0007.md#rfc-0007c-sync) clause for sync command (WI-2026-01-29-012)
- Implement fallback logging to workspace when primary fails (WI-2026-01-29-012)
- Implement `skc sync` command to merge local logs to global (WI-2026-01-29-012)
- Stale fallback warning when local logs >1 hour old (WI-2026-01-29-014)
- ADR-0002 Multi-Target Build Output drafted and accepted (WI-2026-01-29-017)
- ADR-0003 MCP as Primary Agent Interface drafted and accepted (WI-2026-01-29-017)
- C-MCP-OVERVIEW clause for MCP interface overview (WI-2026-01-29-019)
- C-MCP-SERVER clause for server specification (WI-2026-01-29-019)
- C-MCP-TOOLS clause for tool specifications (WI-2026-01-29-019)
- `--target` flag for build command (default: claude) (WI-2026-01-29-020)
- Target registry in config with built-in defaults (WI-2026-01-29-020)
- `skc mcp` command for MCP server (WI-2026-01-29-021)
- MCP tools: skc_outline, skc_show, skc_open, skc_sources, skc_search, skc_stats, skc_build, skc_sync (WI-2026-01-29-021)
- Add missing optional params to MCP tools (WI-2026-01-29-022)
- Create [RFC-0007](docs/rfc/RFC-0007.md) CLI Reference (WI-2026-01-29-023)
- Add C-INIT to [RFC-0006](docs/rfc/RFC-0006.md) for init command (WI-2026-01-29-023)
- `skc init` creates `.skillc/` project structure (WI-2026-01-29-024)
- `skc init <name>` creates project-local skill (WI-2026-01-29-024)
- `skc init <name> --global` creates global skill (WI-2026-01-29-024)
- RFC-0005 amended with warning code specification (WI-2026-01-29-026)
- Warning codes W001-W003 defined for existing warnings (WI-2026-01-29-026)
- Warning infrastructure in error.rs (WI-2026-01-29-026)
- Implement `skc lint` command per [RFC-0008](docs/rfc/RFC-0008.md) (WI-2026-01-29-027)
- Implement SKL1xx frontmatter rules (WI-2026-01-29-027)
- Implement SKL2xx structure rules (WI-2026-01-29-027)
- Implement SKL3xx link rules (WI-2026-01-29-027)
- Implement SKL4xx file rules (WI-2026-01-29-027)
- Implement `skc_init` MCP tool (WI-2026-01-30-001)
- Implement `skc_lint` MCP tool (WI-2026-01-30-001)
- Local source compiles to .skillc/runtime/ (SSOT) and symlinks to agent directory (WI-2026-01-30-005)
- --global flag compiles to ~/.skillc/runtime/ (global SSOT) (WI-2026-01-30-005)
- --target flag specifies which agents to deploy to (default: claude) (WI-2026-01-30-005)
- --copy flag forces copy instead of symlink/junction (WI-2026-01-30-005)
- Cross-platform support: symlink (Unix), junction/copy fallback (Windows) (WI-2026-01-30-005)
- Unit tests for deploy.rs and integration tests for build command (WI-2026-01-30-005)
- Config file parsing for global and project configs (WI-2026-01-30-006)
- Tokenizer preference setting (ascii/cjk) (WI-2026-01-30-006)
- Import flow for direct paths (copy to .skillc/skills/) (WI-2026-01-30-007)
- --force flag for overwriting during import (WI-2026-01-30-007)
- Recursive-up project detection for skill lookup (WI-2026-01-30-007)
- Enhanced build output showing Source/Runtime/Deploy with scope labels (WI-2026-01-30-007)
- CLI list command with text output (WI-2026-01-30-008)
- MCP skc_list tool with JSON output (WI-2026-01-30-008)
- Skill discovery from project and global source stores (WI-2026-01-30-008)
- Status detection (normal, not-built, stale) (WI-2026-01-30-008)
- Filtering by scope, status, limit, pattern (WI-2026-01-30-008)
- Short flags for common options (-g, -f, -o, -t, -l, -p) (WI-2026-01-30-009)
- Colored output using comfy-table (WI-2026-01-30-009)
- Document list command in README (WI-2026-01-30-011)
- Integration tests for lint command (WI-2026-01-30-012)
- Integration tests for list command (WI-2026-01-30-012)
- `skc stats --group-by search` returns search query breakdown (WI-2026-01-30-013)
- `skc outline --level <n>` filters headings by level (WI-2026-01-30-013)
- `skc show --max-lines <n>` limits output lines (WI-2026-01-30-013)
- `skc open --max-lines <n>` limits output lines (WI-2026-01-30-013)

### Changed

- Error messages include error codes (e.g., error[E001]: ...) (WI-2026-01-29-007)
- Sync always deletes local logs after successful upload (remove --purge option) (WI-2026-01-29-014)
- Partial failure handling - delete what synced successfully (WI-2026-01-29-014)
- RFC-0001:C-STUB updated to include MCP preference statement (WI-2026-01-29-018)
- C-OVERVIEW updated to mention MCP (WI-2026-01-29-019)
- Source resolution uses `~/.skillc/skills/` instead of `~/.skillc/src/` (WI-2026-01-29-020)
- Move C-COMMANDS from [RFC-0002](docs/rfc/RFC-0002.md) to [RFC-0007](docs/rfc/RFC-0007.md) (WI-2026-01-29-023)
- Move C-MCP-OVERVIEW from [RFC-0002](docs/rfc/RFC-0002.md) to [RFC-0007](docs/rfc/RFC-0007.md) (WI-2026-01-29-023)
- Move C-MCP-SERVER from [RFC-0002](docs/rfc/RFC-0002.md) to [RFC-0007](docs/rfc/RFC-0007.md) (WI-2026-01-29-023)
- Move C-RESOLUTION from [RFC-0002](docs/rfc/RFC-0002.md) to [RFC-0007](docs/rfc/RFC-0007.md) (WI-2026-01-29-023)
- Move C-LOGGING from [RFC-0002](docs/rfc/RFC-0002.md) to [RFC-0007](docs/rfc/RFC-0007.md) (WI-2026-01-29-023)
- Move C-SYNC from [RFC-0002](docs/rfc/RFC-0002.md) to [RFC-0007](docs/rfc/RFC-0007.md) (WI-2026-01-29-023)
- Remove --source-dir and --runtime-dir CLI flags (WI-2026-01-30-007)
- Remove SKILLC_SOURCE and SKILLC_RUNTIME env var support (WI-2026-01-30-007)
- Remove RFC references from CLI help text (WI-2026-01-30-009)
- Replace regex link extraction with pulldown-cmark AST (WI-2026-01-30-010)

### Removed

- Remove skc_sync from MCP (CLI-only per RFC) (WI-2026-01-29-022)

### Fixed

- Resolver checks runtime store as fallback (WI-2026-01-29-005)
- Error message for invalid project path should be descriptive (WI-2026-01-29-006)
- InvalidPath error shows descriptive message (WI-2026-01-29-007)
- Clippy collapsible if warning in tests/common/mod.rs (WI-2026-01-29-008)
- Source hash is deterministic across rebuilds (excludes .git/.jj directories) (WI-2026-01-29-010)
- Init creates non-empty description per [RFC-0001](docs/rfc/RFC-0001.md) (WI-2026-01-29-025)
- Compiler rejects symlinks escaping skill root per [RFC-0001](docs/rfc/RFC-0001.md) (WI-2026-01-29-025)
- Enforce 100-line stub size limit per [RFC-0001:C-CONSTRAINTS](docs/rfc/RFC-0001.md#rfc-0001c-constraints) (WI-2026-01-30-002)
- Complete staleness check per [RFC-0004:C-INDEX](docs/rfc/RFC-0004.md#rfc-0004c-index) (WI-2026-01-30-002)
- Status filter documentation (stale -> obsolete) (WI-2026-01-30-009)
- Linter SKL301 no longer flags links inside code blocks (WI-2026-01-30-010)
- Linter SKL301 no longer flags links inside inline code (WI-2026-01-30-010)
- Correct storage path from ~/.skillc/src/ to ~/.skillc/skills/ (WI-2026-01-30-011)

### Security

- Add cargo-deny configuration for dependency auditing (WI-2026-01-29-008)
