# Usage Analytics

This guide covers tracking how agents use your skills locally.

## Why analytics?

Understanding usage helps you:

- Identify which sections agents read most
- Find content that's never accessed (candidates for removal)
- Optimize skill structure based on real patterns

## How it works

When you use gateway commands (`skc show`, `skc open`, `skc search`), skillc logs each access:

- Timestamp
- Command used
- Skill and section accessed
- Working directory context

Logs are stored locally in `.skillc-meta/logs.db` within each skill's runtime directory.

## View statistics

```bash
skc stats my-skill
```

Example output:

```
my-skill usage statistics (last 30 days)

Total accesses: 47
Unique sections: 12

Top sections:
  1. Installation (15 accesses)
  2. Troubleshooting (12 accesses)
  3. API Reference (8 accesses)

Unused sections:
  - Contributing
  - Changelog

Access by command:
  show: 35
  search: 8
  open: 4
```

## Filter by time

```bash
skc stats my-skill --since 7d    # Last 7 days
skc stats my-skill --since 30d   # Last 30 days
skc stats my-skill --since 2024-01-01
```

## JSON output

For programmatic analysis:

```bash
skc stats my-skill --format json
```

```json
{
  "skill": "my-skill",
  "period": {
    "start": "2024-01-01T00:00:00Z",
    "end": "2024-01-30T23:59:59Z"
  },
  "total_accesses": 47,
  "sections": [
    { "name": "Installation", "count": 15 },
    { "name": "Troubleshooting", "count": 12 }
  ]
}
```

## Sync local logs

If you work across multiple machines or projects, you may have fragmented logs. Sync them to a central location:

```bash
skc sync
```

This moves local fallback logs to the primary runtime location.

## Privacy

- All analytics are **local only** — nothing is sent externally
- Logs contain access patterns, not content
- You control what to track and what to delete

## Acting on analytics

### High-traffic sections

If a section gets many hits:

- Ensure it's well-written and complete
- Consider expanding with more detail
- Keep it near the top of your skill

### Unused sections

If a section is never accessed:

- Is the heading discoverable? (Check `skc outline`)
- Is the content useful? Consider removing it
- Should it be merged with another section?

### Search patterns

If certain queries appear often:

- Add a section that directly answers them
- Improve headings to match search terms

## Next steps

- Use analytics to iterate on your skill structure
- [Publish updates](./publishing.md) based on usage patterns
