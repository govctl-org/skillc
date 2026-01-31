//! Markdown parsing utilities using pulldown-cmark AST.
//!
//! This module provides proper markdown structure awareness, avoiding
//! false positives from content inside code blocks or inline code.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

/// Strip YAML frontmatter from markdown content.
///
/// Returns the content after the closing `---` delimiter, or the original
/// content if no valid frontmatter is present.
fn strip_frontmatter(content: &str) -> &str {
    // Check for opening delimiter
    let after_open = content
        .strip_prefix("---\r\n")
        .or_else(|| content.strip_prefix("---\n"));

    let Some(after_open) = after_open else {
        return content;
    };

    // Find closing delimiter
    if let Some(close_pos) = after_open.find("\n---") {
        // Return content after the closing delimiter and its newline
        let after_close = &after_open[close_pos + 4..];
        after_close.strip_prefix('\n').unwrap_or(after_close)
    } else {
        content
    }
}

/// Represents a link extracted from markdown content.
#[derive(Debug, Clone)]
pub struct ExtractedLink {
    /// The link destination (URL or path)
    pub dest: String,
    /// Line number (1-indexed) where the link appears
    pub line: usize,
}

/// Extract links from markdown content, excluding those inside code blocks/inline code.
///
/// Uses pulldown-cmark AST to properly understand markdown structure.
pub fn extract_links(content: &str) -> Vec<ExtractedLink> {
    let mut links = Vec::new();
    let mut in_code = false;

    // Enable GFM extensions
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);

    let parser = Parser::new_ext(content, options);

    for (event, range) in parser.into_offset_iter() {
        // Calculate line number from byte offset
        let line = content[..range.start].matches('\n').count() + 1;

        match event {
            // Track when we enter/exit code blocks or inline code
            Event::Start(Tag::CodeBlock(_)) => {
                in_code = true;
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code = false;
            }
            Event::Code(_) => {
                // Inline code is a single event, not start/end
                // We don't need to track it since links can't be nested inside
            }
            // Capture links only when not inside code
            Event::Start(Tag::Link { dest_url, .. }) if !in_code => {
                links.push(ExtractedLink {
                    dest: dest_url.to_string(),
                    line,
                });
            }
            Event::Start(Tag::Image { dest_url, .. }) if !in_code => {
                // Images are also links for file existence checking
                links.push(ExtractedLink {
                    dest: dest_url.to_string(),
                    line,
                });
            }
            _ => {}
        }
    }

    links
}

/// Represents a heading extracted from markdown content.
#[derive(Debug, Clone)]
pub struct ExtractedHeading {
    /// Heading level (1-6)
    pub level: usize,
    /// Heading text content
    pub text: String,
    /// Line number (1-indexed)
    pub line: usize,
}

/// Extract headings from markdown content using AST parsing.
///
/// Uses pulldown-cmark to properly parse markdown structure, avoiding
/// false positives from `#` characters inside code blocks.
///
/// Automatically strips YAML frontmatter before parsing.
pub fn extract_headings(content: &str) -> Vec<ExtractedHeading> {
    // Strip frontmatter to avoid parsing YAML as markdown
    let body = strip_frontmatter(content);
    // Calculate line offset from stripped frontmatter
    let frontmatter_lines = content.len() - body.len();
    let line_offset = content[..frontmatter_lines].matches('\n').count();

    let mut headings = Vec::new();
    let mut in_heading = false;
    let mut current_heading = String::new();
    let mut heading_line = 0;
    let mut heading_level = 0;

    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(body, options);

    for (event, range) in parser.into_offset_iter() {
        let line = body[..range.start].matches('\n').count() + 1 + line_offset;

        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = true;
                current_heading.clear();
                heading_line = line;
                heading_level = level as usize;
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                if !current_heading.is_empty() {
                    headings.push(ExtractedHeading {
                        level: heading_level,
                        text: current_heading.clone(),
                        line: heading_line,
                    });
                }
            }
            Event::Text(text) | Event::Code(text) if in_heading => {
                current_heading.push_str(&text);
            }
            _ => {}
        }
    }

    headings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_links_basic() {
        let content = "# Title\n\n[link](target.md)\n";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].dest, "target.md");
        assert_eq!(links[0].line, 3);
    }

    #[test]
    fn test_extract_links_skips_code_block() {
        let content = r#"# Title

```markdown
[fake link](fake.md)
```

[real link](real.md)
"#;
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].dest, "real.md");
    }

    #[test]
    fn test_extract_links_skips_inline_code() {
        let content = "Use `[text](url)` for links. [real](real.md)\n";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].dest, "real.md");
    }

    #[test]
    fn test_extract_links_in_table() {
        let content = r#"| Col1 | Col2 |
|------|------|
| [link](a.md) | text |
"#;
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].dest, "a.md");
    }

    #[test]
    fn test_extract_links_table_with_code() {
        // This was the false positive case: link syntax inside backticks in a table
        let content = r#"| Format | Markdown |
|--------|----------|
| Link | `[text](url)` |
"#;
        let links = extract_links(content);
        // The `[text](url)` inside backticks should NOT be extracted
        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_links_includes_images() {
        let content = "![image](image.png)\n";
        let links = extract_links(content);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].dest, "image.png");
    }

    #[test]
    fn test_extract_links_image_in_code_block() {
        let content = r#"```markdown
![Alt text](image.png){width=80%}
```
"#;
        let links = extract_links(content);
        assert!(links.is_empty());
    }

    #[test]
    fn test_extract_headings() {
        let content = "# First\n## Second\n### Third\n";
        let headings = extract_headings(content);
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0].text, "First");
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].text, "Second");
        assert_eq!(headings[1].level, 2);
        assert_eq!(headings[2].text, "Third");
        assert_eq!(headings[2].level, 3);
    }

    #[test]
    fn test_extract_headings_skips_code_block() {
        let content = r#"# Real Heading

```bash
# This is a comment, not a heading
echo "hello"
```

## Another Real Heading
"#;
        let headings = extract_headings(content);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].text, "Real Heading");
        assert_eq!(headings[1].text, "Another Real Heading");
    }

    #[test]
    fn test_extract_headings_skips_frontmatter() {
        let content = r#"---
name: my-skill
description: "A skill"
---

# Main Heading

## Sub Heading
"#;
        let headings = extract_headings(content);
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].text, "Main Heading");
        assert_eq!(headings[0].level, 1);
        assert_eq!(headings[1].text, "Sub Heading");
        assert_eq!(headings[1].level, 2);
    }
}
