# Validating Skills

This guide covers using `skc lint` to validate skill quality before publishing.

## Basic usage

```bash
skc lint my-skill
```

This checks your skill against quality rules and reports any issues.

## What lint checks

See [RFC-0008:C-REGISTRY](../rfc/RFC-0008.md#rfc-0008c-registry) for the full rule specification.

### Meta rules (SKL0xx)

- **SKL001**: Skip compiled skills (internal)

### Frontmatter rules (SKL1xx)

- **SKL100**: Frontmatter must be valid YAML
- **SKL101**: `name` field required
- **SKL102**: `name` format (lowercase, hyphens, digits only)
- **SKL103**: `name` length (1-64 chars)
- **SKL104**: `name` should match directory name
- **SKL105**: `description` field required
- **SKL106**: `description` must be non-empty
- **SKL107**: `description` length (10-200 chars recommended)
- **SKL108**: Include activation triggers
- **SKL109**: Only known frontmatter fields allowed

### Structure rules (SKL2xx)

- **SKL201**: SKILL.md size warning (>500 lines)
- **SKL202**: Missing H1 heading
- **SKL203**: H1 should match skill name
- **SKL204**: First heading should be H1
- **SKL205**: No skipped heading levels

### Link rules (SKL3xx)

- **SKL301**: Internal links must resolve
- **SKL302**: Anchor links must resolve
- **SKL303**: Links must not escape skill root

### File rules (SKL4xx)

- **SKL401**: No orphan files (files not linked from SKILL.md)

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
