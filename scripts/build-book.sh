#!/usr/bin/env bash
# Build mdbook from documentation
# Usage: ./scripts/build-book.sh [--serve]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
DOCS_DIR="$PROJECT_ROOT/docs"

cd "$PROJECT_ROOT"

# Parse arguments
SERVE=false
for arg in "$@"; do
    case $arg in
        --serve) SERVE=true ;;
    esac
done

# Render governed documents (RFCs, ADRs) to markdown
echo "Rendering governed documents..."
govctl render all
govctl render changelog

# Copy CHANGELOG.md to docs/ for mdbook
if [[ -f "$PROJECT_ROOT/CHANGELOG.md" ]]; then
    cp "$PROJECT_ROOT/CHANGELOG.md" "$DOCS_DIR/CHANGELOG.md"
fi

# Generate SUMMARY.md dynamically
echo "Generating SUMMARY.md..."

SUMMARY="$DOCS_DIR/SUMMARY.md"
cat > "$SUMMARY" << 'EOF'
# Summary

[Introduction](./INTRODUCTION.md)
EOF

# Add Workflows section
if [[ -d "$DOCS_DIR/workflows" ]] && ls "$DOCS_DIR/workflows/"*.md &>/dev/null; then
    echo "" >> "$SUMMARY"
    echo "# Workflows" >> "$SUMMARY"
    # Define order explicitly for better UX
    for name in authoring validating testing publishing analytics; do
        workflow="$DOCS_DIR/workflows/${name}.md"
        if [[ -f "$workflow" ]]; then
            title=$(grep -m1 '^# ' "$workflow" | sed 's/^# //' || echo "$name")
            echo "- [$title](./workflows/${name}.md)" >> "$SUMMARY"
        fi
    done
fi

echo "" >> "$SUMMARY"
echo "# Specifications" >> "$SUMMARY"

# Add RFCs (sorted)
if [[ -d "$DOCS_DIR/rfc" ]]; then
    for rfc in $(ls "$DOCS_DIR/rfc/"*.md 2>/dev/null | sort); do
        filename=$(basename "$rfc")
        id="${filename%.md}"
        # Extract title from first H1
        title=$(grep -m1 '^# ' "$rfc" | sed 's/^# //' || echo "$id")
        echo "- [$title](./rfc/$filename)" >> "$SUMMARY"
    done
fi

# Add ADRs section if any exist
if [[ -d "$DOCS_DIR/adr" ]] && ls "$DOCS_DIR/adr/"*.md &>/dev/null; then
    echo "" >> "$SUMMARY"
    echo "# Decisions" >> "$SUMMARY"
    for adr in $(ls "$DOCS_DIR/adr/"*.md 2>/dev/null | sort); do
        filename=$(basename "$adr")
        id="${filename%.md}"
        title=$(grep -m1 '^# ' "$adr" | sed 's/^# //' || echo "$id")
        echo "- [$title](./adr/$filename)" >> "$SUMMARY"
    done
fi

# Add Changelog at the end
if [[ -f "$DOCS_DIR/CHANGELOG.md" ]]; then
    echo "" >> "$SUMMARY"
    echo "---" >> "$SUMMARY"
    echo "" >> "$SUMMARY"
    echo "[Changelog](./CHANGELOG.md)" >> "$SUMMARY"
fi

echo "Generated: $SUMMARY"

# Build or serve
cd "$DOCS_DIR"
if [[ "$SERVE" == "true" ]]; then
    echo "Starting mdbook server..."
    mdbook serve --open
else
    echo "Building mdbook..."
    mdbook build
    echo "Book built: $DOCS_DIR/book/"
fi
