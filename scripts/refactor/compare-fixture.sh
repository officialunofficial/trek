#!/usr/bin/env bash
# Compare Trek output to Defuddle's expected output for one or more fixtures.
#
# Usage:
#   scripts/refactor/compare-fixture.sh <fixture-name> [<fixture-name> ...]
#   scripts/refactor/compare-fixture.sh --all     # run the canonical 8-fixture sample
#
# Fixture name = basename without .html, e.g. "general--wikipedia".
# Fixtures live in $DEFUDDLE_FIXTURES (default /tmp/defuddle-clone/tests/fixtures).
# Expected markdown lives in $DEFUDDLE_EXPECTED (default /tmp/defuddle-clone/tests/expected).
#
# Output is written under $OUT_DIR (default /tmp/trek-gap-results):
#   <fixture>.trek.json     -- Trek extraction summary as JSON
#   <fixture>.expected.md   -- Defuddle's golden output (copied for convenience)
#   <fixture>.diff.txt      -- side-by-side comparison

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEFUDDLE_FIXTURES="${DEFUDDLE_FIXTURES:-/tmp/defuddle-clone/tests/fixtures}"
DEFUDDLE_EXPECTED="${DEFUDDLE_EXPECTED:-/tmp/defuddle-clone/tests/expected}"
OUT_DIR="${OUT_DIR:-/tmp/trek-gap-results}"

CANONICAL_SAMPLE=(
  "comments--old.reddit.com-r-test-comments-abc123-test_post"
  "comments--news.ycombinator.com-item-id=12345678"
  "general--substack-app"
  "codeblocks--rehype-pretty-code"
  "general--github.com-issue-56"
  "general--wikipedia"
  "callouts--obsidian-publish-callouts"
  "elements--embedded-videos"
)

mkdir -p "$OUT_DIR"

# Build the example once
(cd "$REPO_ROOT" && cargo build --example extract_file --quiet)

run_one() {
  local name="$1"
  local html="$DEFUDDLE_FIXTURES/${name}.html"
  local expected="$DEFUDDLE_EXPECTED/${name}.md"

  if [[ ! -f "$html" ]]; then
    echo "SKIP: $name (html not found at $html)" >&2
    return 0
  fi

  local trek_out="$OUT_DIR/${name}.trek.json"
  local diff_out="$OUT_DIR/${name}.diff.txt"

  (cd "$REPO_ROOT" && \
    cargo run --example extract_file --quiet -- \
      "$html" "https://example.test/${name}" \
    > "$trek_out") 2>/dev/null

  {
    echo "=============================================="
    echo "FIXTURE: $name"
    echo "=============================================="
    echo ""
    echo "--- TREK output (JSON) ---"
    cat "$trek_out"
    echo ""
    echo ""
    echo "--- DEFUDDLE expected (first 120 lines) ---"
    if [[ -f "$expected" ]]; then
      head -120 "$expected"
      cp "$expected" "$OUT_DIR/${name}.expected.md"
    else
      echo "(expected file not found: $expected)"
    fi
  } > "$diff_out"

  echo "[ok] $name -> $diff_out"
}

if [[ "${1:-}" == "--all" ]]; then
  for f in "${CANONICAL_SAMPLE[@]}"; do
    run_one "$f"
  done
elif [[ $# -eq 0 ]]; then
  echo "usage: $0 <fixture-name> [...]   (or --all)" >&2
  exit 2
else
  for f in "$@"; do
    run_one "$f"
  done
fi

echo ""
echo "Results in: $OUT_DIR"
