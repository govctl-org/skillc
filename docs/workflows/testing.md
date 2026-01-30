# Testing Skills Locally

This guide covers testing skills with gateway commands before publishing.

## Why test locally?

Before publishing, you want to verify:

1. Agents can navigate your skill's structure
2. Content is accessible via gateway commands
3. Search returns relevant results

## Build for local testing

```bash
skc build my-skill
```

This compiles your skill and deploys it to the agent directory (e.g., `~/.claude/skills/`).

**Note:** The compiled output is a stub that references gateway commands. This is for local testing only — you publish the source, not the compiled output.

## Gateway commands

### View structure

```bash
skc outline my-skill
```

Shows all sections in your skill:

```
# my-skill
## Installation
## Usage
### Basic usage
### Advanced options
## Troubleshooting
```

### Read a section

```bash
skc show my-skill --section "Installation"
```

Displays the content of that section. This simulates how an agent would request specific content.

### Open a file

```bash
skc open my-skill examples/usage.md
```

Opens a specific file from the skill. Use this for non-markdown files or when you need the entire file.

### List source files

```bash
skc sources my-skill
```

Lists all files in the skill:

```
SKILL.md
examples/usage.md
reference/api.md
```

### Search content

```bash
skc search my-skill "error handling"
```

Full-text search across all skill content:

```
examples/usage.md:45: ...proper error handling is crucial...
reference/api.md:112: ...the error handling middleware...
```

## Test with an agent

After building, start a conversation with an agent that has access to `~/.claude/skills/` (or your target agent directory).

Ask the agent to use your skill and observe:

1. Does it find the skill?
2. Does it navigate to relevant sections?
3. Does it understand the content?

## Deploy to multiple agents

Test with different agents:

```bash
skc build my-skill --target claude,cursor,codex
```

## Iterate

1. Edit source files in `.skillc/skills/my-skill/`
2. Rebuild: `skc build my-skill`
3. Test with gateway commands or agent
4. Repeat

No need to restart agents — they read fresh content on each request.

## Next steps

- [Check usage analytics](./analytics.md) to see what agents access
- [Publish](./publishing.md) when testing is complete
