# Track A — Fixture-Based Test Harness (Defuddle Parity)

Status: spec only. No production code in this track.

## Goal

Port Defuddle's fixture-based regression suite into Trek so we can use
Defuddle's gold-master extraction outputs as our oracle while we evolve
Trek toward parity.

The Defuddle harness (`tests/fixtures.test.ts`):

1. Discovers every `tests/fixtures/*.html`.
2. Pulls the URL from a JSON frontmatter comment
   `<!-- {"url":"..."} -->`, else falls back to the filename.
3. Runs Defuddle with `{ separateMarkdown: true }`.
4. Builds a comparable string: a fenced JSON preamble containing
   `title`, `author`, `site`, `published`, then the markdown body.
5. Diffs that string against `tests/expected/<name>.md`. Also diffs
   `response.content` against `tests/expected/<name>.html` if present.

Trek must reproduce this byte-for-byte so we can drop Defuddle's
`expected/*.md` in unmodified and use mismatch as the work queue.

---

## 1. Vendor vs. submodule

**Decision: vendor.** Copy `tests/fixtures/` and `tests/expected/` into
Trek's tree, do not git-submodule.

Reasons:

- Small enough to live in-tree: `tests/fixtures/` ≈ **1.9 MB / 187
  `.html`** files; `tests/expected/` ≈ **968 KB / 190 files** (187 `.md`
  + 3 `.html`).
- Trek will diverge on some fixtures during the port; we need to edit
  expected files locally without upstream churn.
- Submodule pinning adds contributor and CI friction.
- Record Defuddle's commit SHA in `tests/fixtures/SOURCE.md` for resync.

### Destination paths

Source (Defuddle clone)                         → Destination (Trek)
- `/tmp/defuddle-clone/tests/fixtures/*.html`   → `tests/fixtures/*.html`
- `/tmp/defuddle-clone/tests/expected/*.md`     → `tests/expected/*.md`
- `/tmp/defuddle-clone/tests/expected/*.html`   → `tests/expected/*.html`
                                                  (only 3: `footnotes--numeric-anchor-id.html`,
                                                  `issues--sidebar-toggle-checkbox.html`,
                                                  `small-images--svg-icon-viewbox.html`)

Add `tests/fixtures/SOURCE.md` recording: upstream repo URL, commit SHA,
copy date, the `cp -R` command used.

**Frontmatter coverage:** 67 of 187 fixtures contain a
`<!-- {"url":"..."} -->` comment; the other 120 must use the
filename-derived URL fallback (see §2.3).

---

## 2. Rust test runner design

One new integration test file: **`tests/fixtures_test.rs`**, plus a small
helper module **`tests/common/mod.rs`**.

### 2.1 Discovery

```rust
// tests/common/mod.rs
pub struct Fixture {
    pub name: String,        // basename without .html
    pub html_path: PathBuf,  // tests/fixtures/<name>.html
    pub expected_md_path: PathBuf,   // tests/expected/<name>.md
    pub expected_html_path: PathBuf, // tests/expected/<name>.html (may not exist)
}

pub fn discover_fixtures() -> Vec<Fixture>;
```

Implementation: `walkdir::WalkDir::new("tests/fixtures").max_depth(1)`,
filter `.html`, sorted by name for deterministic output. Resolve paths
relative to `env!("CARGO_MANIFEST_DIR")`.

### 2.2 Per-fixture test generation

`#[test]` per fixture is impossible without a build script or a macro. Two
acceptable patterns; pick **(A)** for v1.

**(A) Single parameterized test that iterates and reports per-fixture
failures.** A failure inside the loop pushes onto a `Vec<FixtureFailure>`
and at the end we `panic!` with a summary if any entry exists. This loses
`cargo test` per-test granularity but is simple, deterministic, and
matches `vitest test.each` semantics closely enough.

```rust
#[test]
fn fixtures_match_expected() { ... }
```

**(B) Future:** a `build.rs` that codegens one `#[test] fn
fixture_<safe_name>()` per fixture into `OUT_DIR`. Defer until v1.

### 2.3 Frontmatter URL parsing

Match Defuddle exactly:

```rust
// once_cell + regex (regex already in deps)
static URL_FRONTMATTER: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"<!--\s*(\{"url":.*?\})\s*-->"#).unwrap());

fn extract_url(html: &str, fixture_name: &str) -> String {
    if let Some(caps) = URL_FRONTMATTER.captures(html) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&caps[1]) {
            if let Some(u) = v.get("url").and_then(|x| x.as_str()) {
                return u.to_string();
            }
        }
    }
    // Defuddle fallback: strip leading "<lowercase>--" prefix from name,
    // then prepend "https://"
    let stripped = Regex::new(r"^[a-z]+--").unwrap()
        .replace(fixture_name, "");
    format!("https://{stripped}")
}
```

### 2.4 Calling Trek

```rust
let opts = TrekOptions {
    url: Some(url.clone()),
    output: OutputOptions {
        markdown: false,
        separate_markdown: true,   // see §5: must add this codepath
    },
    ..TrekOptions::default()
};
let trek = Trek::new(opts);
let response = trek.parse(&html)?;
```

### 2.5 Comparable result construction

Mirror Defuddle's `createComparableResult`:

```rust
fn comparable_result(r: &TrekResponse) -> String {
    let metadata = serde_json::json!({
        "title":     r.metadata.title,
        "author":    r.metadata.author,
        "site":      r.metadata.site,        // see §5
        "published": r.metadata.published,
    });
    let preamble = format!(
        "```json\n{}\n```\n\n",
        serde_json::to_string_pretty(&metadata).unwrap()
    );
    let body = r.content_markdown.clone().unwrap_or_default();
    preamble + &body
}
```

Critical: `serde_json::to_string_pretty` must emit 2-space indent and
preserve insertion order of the four keys. Use a `serde_json::Map` (or a
typed struct with `#[derive(Serialize)]` and the four fields in the same
order as Defuddle: title, author, site, published) to guarantee key order.
Empty strings serialize as `""`, matching Defuddle's `JSON.stringify`
behavior for absent string fields. **Do not** convert empty strings to
`null` — Defuddle leaves them as empty strings via the metadata object.

### 2.6 Diff strategy

Use `pretty_assertions::assert_eq!(actual.trim(), expected.trim())` for
colored line diffs. Wrap each fixture in `std::panic::catch_unwind` so
one mismatch doesn't stop the run; collect failures and panic at the end
with a `name → first-divergent-line` summary. If `expected_html_path`
exists, also compare `response.content.trim()` to it; failures reported
under `<name> [html]`.

### 2.7 Snapshot / update mode

Trigger with env var `TREK_UPDATE_FIXTURES=1`:

- If set, *and* the actual result differs from expected (or expected is
  missing), write the actual result to `tests/expected/<name>.md` and
  log `WROTE <name>` to stderr. Test still passes.
- If unset, missing-expected is a **failure** (do *not* silently baseline
  in CI — Defuddle silently baselines, but we want stricter behavior so
  unrelated PRs can't add fixtures without acknowledged baselines).

---

## 3. Markdown gap strategy

Trek currently has no markdown serializer. `TrekResponse.content_markdown`
exists as `Option<String>` but is never populated (`lib.rs` always sets it
to `None`).

### 3.1 Phasing

We will land the harness *before* the markdown serializer (Track B). To
keep CI green:

1. Add a Cargo feature **`markdown-fixtures`** (default: **off**). When
   off, `fixtures_match_expected` is gated `#[cfg(feature =
   "markdown-fixtures")]` and effectively skipped.
2. Add a sibling test **`fixtures_metadata_only`** that runs unconditionally
   and validates only the JSON preamble fields against expected (parse the
   `.md` expected file, extract its leading ```json fence, compare the
   four keys). This gives us value from day one — see §4.
3. When Track B (markdown) lands, flip the feature to default-on in
   `Cargo.toml` and remove the gate.

Until the feature is on, `make test-fixtures` runs only the metadata-only
variant. `make test-fixtures-full` runs with `--features markdown-fixtures`.

### 3.2 Why a feature flag, not `#[ignore]`

`#[ignore]` still compiles and runs the body (failing on the missing
markdown). A feature flag elides the comparison codepath entirely until
the producer side exists, avoiding noise.

---

## 4. Partial-pass model (metadata-only first)

Until markdown is online, run **`fixtures_metadata_only`** as the actual
gate. Algorithm:

1. For each fixture, parse the expected `.md` file: read the leading
   ```json…``` fence.
2. `serde_json::from_str` it into a struct with `title/author/site/published`.
3. Compare each field to the corresponding `TrekResponse.metadata` field.
4. Empty-string equivalence: treat Defuddle `""` and Trek `""` as equal;
   missing field on either side is also `""`.

### 4.1 Fixture tiers

- **Tier 0 (must pass first):** generic content (`general--*`,
  `content-patterns--*`, `headings--*`, ~80 fixtures). Metadata + body
  extraction; no site-specific extractors.
- **Tier 1:** code-block heavy (`codeblocks--*`, `code-blocks--*`).
  Metadata should pass; full markdown waits for Track B.
- **Tier 2:** site-specific (`comments--mastodon.social-*`,
  `comments--news.ycombinator.com-*`, `comments--old.reddit.com-*`).
  Expected-fail until Track C lands per-site extractors.

Add `tests/fixtures/TIERS.toml`:

```toml
[tier0]
include = ["general--*", "content-patterns--*", "headings--*", "author-*", "table-layout--*"]

[tier1]
include = ["codeblocks--*", "code-blocks--*", "callouts--*"]

[tier2]
include = ["comments--*", "footnotes--*"]

[skip]
# Fixtures known to require features Trek does not yet implement.
# Format: "<name>" = "<reason>"
"comments--old.reddit.com-r-test-comments-abc123-test_post" = "needs reddit extractor"
```

The runner reads this and tags each fixture with its tier; tier-2 and
explicit `[skip]` entries are recorded as `expected_fail` and don't
contribute to the failure count, but are listed in the summary so we know
the queue.

---

## 5. Required `TrekResponse` / `TrekMetadata` changes

Inspecting `src/types.rs`:

- `TrekMetadata.site: String` — **already present** (line 93). No change.
- `TrekMetadata.published: String` — **already present** (line 87). No change.
- `TrekResponse.content_markdown: Option<String>` — **already present**
  (line 127), currently always `None`. Track B fills it.
- `TrekOptions.output.separate_markdown: bool` — **already present**
  (line 33). Track B must honor it (currently ignored — `parse_internal`
  always sets `content_markdown: None`).

### Behavioral changes needed in `src/lib.rs` (Track B, called out here so
the harness contract is concrete):

- When `options.output.separate_markdown` is `true`, populate
  `TrekResponse.content_markdown = Some(html_to_markdown(&final_content))`.
- `metadata.site` should be filled from `og:site_name` first, then
  Schema.org `publisher.name` / `isPartOf.name`, falling back to a
  derived domain. Currently `MetadataExtractor` likely doesn't set
  `site`; verify and extend.
- `metadata.published` must serialize as Defuddle does. Sample expected:
  `"published": "2025-03-19T00:00:00+00:00"` — i.e. ISO-8601 with offset.
  Trek currently passes `published` through as-extracted (e.g.
  `"2024-01-01"`). Add a normalization step (RFC 3339 / ISO 8601 with
  `+00:00`) when the source is a bare date.

These three behavioral notes are the only `types.rs` / `lib.rs` deltas the
harness cares about. **No type fields need to be added.**

---

## 6. Cargo.toml changes

Add to `[dev-dependencies]`:

```toml
[dev-dependencies]
wasm-bindgen-test = "0.3"      # already present
pretty_assertions = "1.4"      # NEW: colored diff output
walkdir = "2.5"                # NEW: fixture discovery
# serde_json already in [dependencies], reused
# regex already in [dependencies], reused
# once_cell already in [dependencies], reused
```

Add new feature:

```toml
[features]
default = []
# Run the full fixture suite (markdown body diff). Off until Track B lands.
markdown-fixtures = []
```

No production-side dependency changes.

---

## 7. Files to create

```
tests/fixtures/                              # vendored .html fixtures (187 files, ~1.9MB)
tests/fixtures/SOURCE.md                     # upstream commit SHA + copy date
tests/fixtures/TIERS.toml                    # tier classification + skip list
tests/expected/                              # vendored expected outputs (190 files, ~968KB)
tests/common/mod.rs                          # Fixture struct, discover_fixtures(), comparable_result(), URL parser
tests/fixtures_test.rs                       # the two test entry points: fixtures_metadata_only + (gated) fixtures_match_expected
docs/refactor/track-a-test-harness.md        # this doc
```

Minimal one-liners:

- `tests/common/mod.rs` — shared helpers: discovery, URL extraction,
  comparable-string builder, expected-file readers, tier loader.
- `tests/fixtures_test.rs` — two `#[test]` functions, both iterate
  `discover_fixtures()` and aggregate failures; one feature-gated.
- `tests/fixtures/TIERS.toml` — declarative tier + skip configuration.
- `tests/fixtures/SOURCE.md` — provenance for the vendored data.

---

## 8. Makefile targets

Append to `Makefile`:

```make
.PHONY: test-fixtures
test-fixtures: ## Run fixture-based regression tests (metadata-only until markdown lands)
	@echo "$(YELLOW)Running fixture tests (metadata only)...$(NC)"
	$(CARGO) test --test fixtures_test fixtures_metadata_only -- --nocapture

.PHONY: test-fixtures-full
test-fixtures-full: ## Run full fixture diff including markdown body (requires Track B)
	@echo "$(YELLOW)Running fixture tests (full)...$(NC)"
	$(CARGO) test --test fixtures_test --features markdown-fixtures -- --nocapture

.PHONY: update-fixtures
update-fixtures: ## Regenerate tests/expected/*.md from current Trek output
	@echo "$(YELLOW)Updating expected fixtures...$(NC)"
	TREK_UPDATE_FIXTURES=1 $(CARGO) test --test fixtures_test --features markdown-fixtures -- --nocapture
	@echo "$(GREEN)Review changes with: git diff tests/expected$(NC)"
```

`make pre-commit` and `make ci` should append `test-fixtures` (not
`test-fixtures-full`) so default CI runs the metadata gate.

---

## Acceptance criteria for Track A

1. `make test-fixtures` passes on `main` with no fixture failures in
   tier 0 (generic + content-patterns) for the metadata-only fields.
2. Tier-1 / tier-2 / skipped fixtures are reported in the summary as
   "expected-fail / skipped" with reasons, and do not count as failures.
3. `TREK_UPDATE_FIXTURES=1 make update-fixtures` rewrites
   `tests/expected/*.md` and produces a clean diff (only the cells we
   know we differ on).
4. Adding a new `.html` to `tests/fixtures/` without a corresponding
   `tests/expected/<name>.md` causes a hard test failure unless
   `TREK_UPDATE_FIXTURES=1` is set.
5. The fenced JSON preamble byte-for-byte matches Defuddle's format
   (2-space indent, key order title/author/site/published, empty strings
   not nulls), validated by passing on at least one Tier-0 fixture once
   `metadata.published` ISO-8601 normalization (Track B prerequisite) is
   in place.
