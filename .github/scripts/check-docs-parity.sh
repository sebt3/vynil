#!/usr/bin/env bash
# Checks that every .md page present in docs/ (English, source of truth) has a
# counterpart in docs/fr/ (French mirror), and vice versa.
# Exit 1 if any mismatch is found.
set -euo pipefail

DOCS_EN="docs"
DOCS_FR="docs/fr"

# Pages to exclude from parity check (internal/drafts not part of the canonical set)
EXCLUDE_PATTERN="backup-evolution|static-index-proposal"

collect_pages() {
  local dir="$1"
  find "$dir" -name "*.md" -not -path "*/fr/*" \
    | sed "s|^$dir/||" \
    | grep -Ev "$EXCLUDE_PATTERN" \
    | sort
}

collect_fr_pages() {
  local dir="$1"
  find "$dir" -name "*.md" \
    | sed "s|^$dir/||" \
    | sort
}

en_pages=$(collect_pages "$DOCS_EN")
fr_pages=$(collect_fr_pages "$DOCS_FR")

errors=0

while IFS= read -r page; do
  if ! echo "$fr_pages" | grep -qx "$page"; then
    echo "MISSING in docs/fr/: $page"
    errors=$((errors + 1))
  fi
done <<< "$en_pages"

while IFS= read -r page; do
  if ! echo "$en_pages" | grep -qx "$page"; then
    echo "MISSING in docs/ (EN): $page"
    errors=$((errors + 1))
  fi
done <<< "$fr_pages"

if [ "$errors" -gt 0 ]; then
  echo ""
  echo "$errors parity error(s) found. Every page must exist in both docs/ (EN) and docs/fr/ (FR)."
  exit 1
fi

echo "OK: docs/ and docs/fr/ are in parity ($( echo "$en_pages" | wc -l | tr -d ' ') pages each)."
