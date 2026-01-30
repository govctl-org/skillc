# Validating Skills

This guide covers using `skc lint` to validate skill quality before publishing.

## Basic usage

```bash
skc lint my-skill
```

This checks your skill against quality rules and reports any issues.

## What lint checks

### Structure rules (SKL0xx)

- **SKL001**: Skip compiled skills (internal)
- **SKL002**: `SKILL.md` must exist at skill root
- **SKL003**: No empty directories

### File rules (SKL1xx)

- **SKL101**: Frontmatter must be valid YAML
- **SKL102**: `name` field required in frontmatter
- **SKL103**: `description` field required in frontmatter
- **SKL104**: `name` must match directory name
- **SKL105**: `description` should be 10-200 characters
- **SKL106**: Title heading should match skill name
- **SKL107**: Skill should have activation triggers
- **SKL108**: `description` shouldn't start with "A skill..."
- **SKL109**: Only known frontmatter fields allowed

### Markdown rules (SKL2xx)

- **SKL201**: Headings should use ATX style (`#`)
- **SKL202**: Single H1 per file
- **SKL203**: No skipped heading levels
- **SKL204**: No trailing punctuation in headings

### Link rules (SKL3xx)

- **SKL301**: Internal links must resolve
- **SKL302**: Anchor links must resolve

## Output formats

### Text (default)

```bash
skc lint my-skill
```

```
my-skill: 2 errors, 1 warning

SKILL.md:1:1 error[SKL103]: missing required frontmatter field: description
SKILL.md:5:1 warning[SKL107]: skill should include activation triggers
examples/usage.md:12:1 error[SKL301]: broken link: ./nonexistent.md
```

### JSON

```bash
skc lint my-skill --format json
```

```json
{
  "skill": "my-skill",
  "diagnostics": [
    {
      "rule": "SKL103",
      "severity": "error",
      "file": "SKILL.md",
      "line": 1,
      "message": "missing required frontmatter field: description"
    }
  ]
}
```

## Fixing issues

### Missing frontmatter fields

Add required fields to your `SKILL.md`:

```yaml
---
name: my-skill
description: Helps with X by providing Y
---
```

### Missing activation triggers

Add a section explaining when agents should use the skill:

```markdown
## When to use this skill

Use this skill when:

- Condition A
- Condition B
```

Or use frontmatter:

```yaml
---
name: my-skill
description: ...
activate when: working with Rust projects
---
```

### Broken links

Check that all internal links point to existing files:

```markdown
<!-- Bad -->

[See examples](./examples.md) <!-- file doesn't exist -->

<!-- Good -->

[See examples](./examples/usage.md) <!-- file exists -->
```

## Lint all skills

To lint all skills in your project:

```bash
skc lint
```

## Exit codes

- `0` — No errors (warnings are OK)
- `1` — One or more errors found

Use exit codes in CI to block publishing of invalid skills.

## Next steps

- [Test locally](./testing.md) with gateway commands
- [Publish](./publishing.md) when validation passes
