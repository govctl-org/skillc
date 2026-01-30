# Contributing to skillc

Thank you for your interest in contributing to skillc! This document provides guidelines and instructions for contributing.

## Getting Started

### Prerequisites

- Rust 1.85 or later (we use Edition 2024)
- Just (command runner)
- pre-commit (for git hooks)

### Development Setup

1. **Clone the repository**

```bash
git clone https://github.com/govctl-org/skillc
cd skillc
```

2. **Install development tools**

```bash
# Install Just
cargo install just

# Install pre-commit hooks
pre-commit install
```

3. **Build the project**

```bash
just build
```

## Development Commands

| Command                 | Description                   |
| ----------------------- | ----------------------------- |
| `just build`            | Build release binary          |
| `just test`             | Run all tests                 |
| `just lint`             | Run clippy lints              |
| `just fmt`              | Format code                   |
| `just pre-commit`       | Run all pre-commit hooks      |
| `just update-snapshots` | Update test snapshots         |
| `just deny`             | Run cargo-deny security audit |
| `just check`            | Validate governed documents   |
| `just render`           | Render RFCs to markdown       |

## Code Style

- **Formatting**: Code is formatted with `cargo fmt`
- **Linting**: Clippy runs with `cargo clippy --workspace -- -D warnings` (warnings are errors)
- **Testing**: All tests must pass before merging

## Commit Messages

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

| Prefix            | Description                  |
| ----------------- | ---------------------------- |
| `feat(scope)`     | New feature                  |
| `fix(scope)`      | Bug fix                      |
| `docs(scope)`     | Documentation changes        |
| `test(scope)`     | Test additions/modifications |
| `refactor(scope)` | Code restructuring           |
| `chore(scope)`    | Maintenance tasks            |

Examples:

```
feat(compiler): add progressive disclosure support
fix(resolver): handle symlinks in skill paths
docs(readme): update installation instructions
```

## Governance

This project uses a governed workflow based on RFCs and work items:

1. **Work Items**: All changes are tracked in `gov/work/`
2. **RFCs**: New features require RFCs in `gov/rfc/`
3. **ADRs**: Architectural decisions are recorded in `gov/adr/`

Use `govctl` commands to manage governance:

```bash
govctl work list pending  # List work items
govctl rfc list           # List RFCs
govctl check              # Validate all governed documents
```

## Pull Requests

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run `just pre-commit` to verify all checks pass
5. Submit a pull request

## Security

- Do not commit secrets or credentials
- Use `cargo deny` to check for vulnerable dependencies
- Report security issues via GitHub Security Advisories

## Questions?

- Check the [README](README.md) for project overview
- Review RFCs in `docs/rfc/` for protocol specifications
- Open an issue for questions or suggestions
