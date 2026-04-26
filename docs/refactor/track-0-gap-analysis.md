# Track 0 — Trek vs Defuddle Gap Analysis

Cross-cutting recon for the Trek refactor. Dated against Trek `0.2.1` and the
checkout of Defuddle at `/tmp/defuddle-clone` (commit unspecified by caller).

This doc is **not** a design proposal. It is a fact sheet other tracks can plan
against.

## 1. Build & test status

### Native test suite

`cargo test --no-fail-fast` from a clean compile:

- Unit tests: **37 passed, 0 failed** (`src/lib.rs` deps).
- Integration tests: **5 passed, 0 failed** (`tests/integration_test.rs`).
- Doc-tests: 0 (none defined).
- Wall-clock: ~9.5s compile, ~0.02s tests.

No flakes, no skips. Trek's existing suite is healthy. Note: the suite tests
internal helpers and a couple of synthetic HTML strings — it does **not**
exercise any of the Defuddle fixtures, which is exactly why this gap analysis
is necessary.

### WASM build

Pre-built artifact already present at `pkg/trek_rs_bg.wasm` from commit
`9987b31` (2025-06-28 — Mini App support PR). Did not rebuild during this
recon. Size:

| Artifact | Bytes | KB | MB |
|---|---|---|---|
| `pkg/trek_rs_bg.wasm` | 1,843,158 | 1,800 | **1.76 MB** |
| `pkg/trek_rs.js` (loader) | 26,691 | 26 | 0.025 |

**Cloudflare Worker free-tier compressed limit is 3 MB; the paid limit is 10 MB.
The 1 MB you mentioned is the Workers _free-plan startup CPU_ envelope, not a
size cap.** The 1.76 MB raw WASM gzips to roughly 600–700 KB (typical
~3:1 ratio for `opt-level="z"` builds), which fits comfortably under the 3 MB
free-tier compressed cap with a lot of headroom for the new modules this
refactor adds (markdown converter, ~25 site extractors, callouts/footnotes
expansion).

If size becomes a concern as we grow, Trek already enables `opt-level="z"`,
`lto = true`, `codegen-units = 1`, `strip = true` (Cargo.toml:64-67), so the
remaining levers are: (a) feature-gate site extractors, (b) drop `color-eyre`
from the WASM target, (c) replace `regex` with `regex-lite` where possible.
None of those need to happen before the refactor.

### Side-channel build

`cargo build --example extract_file` (the new tooling binary added during this
recon — see `examples/extract_file.rs`): clean, 0.74s incremental compile.

## 2. Defuddle fixture corpus

- 187 HTML fixtures in `/tmp/defuddle-clone/tests/fixtures/`.
- Matching golden Markdown in `/tmp/defuddle-clone/tests/expected/` (note: this
  is a **sibling** directory of `fixtures/`, not nested as the original task
  brief implied).

Defuddle's source ships **25 site-specific extractors** totaling 6,576 LOC
(`/tmp/defuddle-clone/src/extractors/*.ts`). Trek currently ships **1**: the
`GenericExtractor` registered in `src/lib.rs:54`.

Defuddle's markdown converter is a single 813-LOC file:
`/tmp/defuddle-clone/src/markdown.ts`. Trek has **no markdown converter** —
when `markdown: true` is requested, `content_markdown` comes back `null`
(verified across all 8 fixtures below; see `extract_file.rs` output in
`/tmp/trek-gap-results/*.trek.json`).

## 3. Per-fixture comparison (8 fixtures, representative sample)

Run via `scripts/refactor/compare-fixture.sh --all`. Raw output in
`/tmp/trek-gap-results/`.

Scoring legend per cell:
- **OK** = matches golden
- **Partial** = present but wrong/dirty
- **Miss** = absent
- **Wrong** = incorrect value
- **Trash** = present but full of unrelated noise (CSS classes, chrome, etc.)

| # | Fixture | Title | Author | Site | Published | Body present | Body shape | Markdown |
|---|---|---|---|---|---|---|---|---|
| 1 | `comments--old.reddit.com-...test_post` | Wrong (`Test Post : test` vs `Test Post`) | Miss (got `""` vs `poster_user`) | Miss (`""` vs `r/test`) | Miss (`""` vs `2025-01-15T10:30:00Z`) | Partial | Trash (raw `tagline`/`score` HTML, no `> Comments` blockquote tree) | Miss (null) |
| 2 | `comments--news.ycombinator.com-...12345678` | Wrong (`A Sample Article \| Hacker News` vs `A Sample Article`) | Miss (`""` vs `author_one`) | Miss (`""` vs `Hacker News`) | Miss (`""` vs `2025-01-15`) | Partial | Trash (raw `<table id="hnmain">`, all `s.gif` indents) | Miss |
| 3 | `general--substack-app` | OK (`Rich Holmes (@richholmes)`) | Wrong (`Rich Holmes` vs `Substack`) | OK (`Substack`) | OK (both empty) | Partial | Trash (~45 KB of `pencraft pc-…` div soup, no list-ordering) | Miss |
| 4 | `codeblocks--rehype-pretty-code` | OK | OK (`Dan Abramov`) | Miss (`""` vs `Dan Abramov`) | OK (empty) | OK | Partial (raw shiki `<figure data-rehype-pretty-code-figure>`, not ` ```fish ` fences) | Miss |
| 5 | `general--github.com-issue-56` | Wrong (missing `· Issue #56` suffix) | OK (`jmorrell`) | OK (`GitHub`) | OK (`2025-05-25T20:35:48.000Z`) | Trash | Trash (65 KB of GitHub UI chrome, `react-partial`, skip-to-content links, "Constants loaded at…" string, etc.) | Miss |
| 6 | `general--wikipedia` | Wrong (`Obsidian (software) - Wikipedia`, suffix not stripped) | OK (empty) | Miss (`""` vs `Wikipedia`) | Miss (`""` vs `2024-01-15T00:00:00+00:00`) | Partial | Trash (full infobox, raw `mw-` chrome) | Miss |
| 7 | `callouts--obsidian-publish-callouts` | OK | OK (empty) | OK (`Example Help`) | OK (empty) | Partial | Wrong (callouts emitted as raw `<svg>` icons + naked text, not `> [!info]` markdown syntax) | Miss |
| 8 | `elements--embedded-videos` | OK | OK | OK | OK | OK | Wrong (`<iframe>` for YouTube/Twitter kept verbatim instead of converted to `![](https://www.youtube.com/watch?v=…)`); Vimeo handling matches expected | Miss |

**Aggregate scores (out of 8):**

| Field | OK | Partial / Wrong / Miss |
|---|---|---|
| Title | 3 | 5 |
| Author | 4 | 4 |
| Site | 3 | 5 |
| Published | 3 | 5 |
| Body present at all | 7 | 1 trash-only |
| Body matches expected shape | 0 | 8 (every body needs more transformations) |
| Markdown output | 0 | 8 (feature unimplemented) |

## 4. Top-10 highest-leverage gaps (priority order)

Priority is set by **how many fixtures the gap blocks** in our sample, weighted
by how many additional fixtures across the 187-fixture corpus the same fix
unblocks (rough estimate from filename prefixes).

| # | Gap | Trek state | Fixtures blocked in sample | Est. corpus reach | Approx LOC budget |
|---|---|---|---|---|---|
| **1** | **HTML→Markdown converter** | Missing entirely. `OutputOptions.markdown` is read but no implementation; `content_markdown` always `null`. | All 8 (every golden file is markdown) | All 187 | 600–900 LOC. Defuddle's is 813 LOC (`src/markdown.ts`). |
| **2** | **Site-extractor registry actually wired** | Registry exists (`src/extractor.rs`), but only `GenericExtractor` registered (`src/lib.rs:54`). No URL-pattern dispatch. | 1, 2, 3, 5, 6 | ~40 fixtures named `general--<site>`, `comments--<site>` | ~50 LOC infra + per-site below. |
| **3** | **Reddit + Hacker News conversation extractors** | Missing. Both currently fall through to Generic, which emits raw `<table>` / `<div class="tagline">` HTML. | 1, 2 | ~3 directly, but format is reused for Mastodon/Discourse/Bluesky/Threads | Reddit 231 LOC + HN 296 LOC + shared `_conversation.ts` 86 LOC ≈ 600 LOC TS → ~700 LOC Rust. |
| **4** | **Code-block normalization** (shiki / rehype-pretty-code / chroma / hljs / mintlify) | Trek preserves whatever wrapper markup the source used; Defuddle collapses ~12 dialects into clean `<pre><code class="language-x">`. | 4 directly; 17 fixture filenames begin with `code-blocks--`/`codeblocks--` | ~17 | Defuddle `elements/code.ts` = 514 LOC. |
| **5** | **Title cleaning** (strip ` - Wikipedia`, ` \| Hacker News`, ` · Issue #N`) | No title-suffix stripping logic. `MetadataExtractor` returns the raw `<title>`. | 2, 5, 6 | ~30 (any site that brands its title) | <80 LOC; can ride on extractor-specific rules. |
| **6** | **Substack content selection** | Falls through to Generic, picks the entire feed sidebar (45 KB of `pencraft pc-*` div soup). | 3 | 4 substack fixtures, plus Medium/LinkedIn likely similar | Defuddle `extractors/substack.ts` = 211 LOC. |
| **7** | **GitHub issue/PR extractor** | Generic dumps the entire chrome (65 KB output, 11× the expected size). | 5 | 2 directly, plus any `issues--…` fixtures referencing GH renders | Defuddle `extractors/github.ts` = 293 LOC. |
| **8** | **Wikipedia extractor** (strip infobox, citation rewriting, footnote linking) | Trek keeps full `infobox vevent` table verbatim; footnotes emitted as `<sup>` not `[^1]`. | 6, plus implicitly footnote-test fixtures | 4 wikipedia + 2 wikidot + footnote tests | Defuddle wikipedia extractor itself is only 24 LOC; the heavy lifting is in `elements/footnotes.ts` (1235 LOC) and metadata-block stripping. |
| **9** | **Embed transformation** (YouTube / Twitter / X iframes → image placeholders) | All `<iframe>` tags are kept verbatim. Defuddle rewrites known providers to `<img src="…watch?v=ID">` so markdown becomes `![](url)`; non-allowlisted (Vimeo) stays as iframe. | 8 | 4 fixtures named `elements--embedded-…`, plus inline embeds in other fixtures | ~150 LOC + a provider allowlist. |
| **10** | **Callout transformation** (Obsidian Publish / Bootstrap alerts / admonitions) | Trek renders the `<svg>` icon + raw text inline; the source `[!info]` semantic is lost. | 7 | 4 `callouts--…` + Bootstrap alerts + Hugo admonitions | Defuddle `elements/callouts.ts` = 138 LOC (small!). |

### Honourable mentions (didn't make top 10 but cheap and visible)

- `<react-partial>` / `<tool-tip>` / `<template>` custom-element stripping —
  trivial selectors, would clean GitHub & React-SSR fixtures meaningfully.
- "Constants loaded at 2025-…" stray text in the GitHub fixture — Trek's
  scoring isn't excluding the SSR bootstrap script content. One regex.
- `class="mw-…"`, `class="pencraft pc-…"` wrapper-flattening: huge byte
  reduction, the patterns are very regular.

## 5. Quick wins (fixes <50 LOC each)

These are gaps where the fix is mechanical and would improve fixture pass
rates without requiring any new architectural work. Numbers reference the
gap-table above.

1. **Title suffix stripping** (gap #5, ~30 LOC): a list of regex pairs
   `(domain_pattern, title_regex_to_strip)`, applied in `MetadataExtractor`
   after `<title>` parse. Handles Wikipedia, Hacker News, GitHub, NYTimes,
   etc. in one place. Unblocks fixtures 2, 5, 6 metadata immediately.

2. **`<iframe>` provider rewrite** (gap #9, ~40 LOC): one match table —
   `youtube.com/embed/ID` → `https://www.youtube.com/watch?v=ID`,
   `youtube-nocookie.com/embed/ID` → same, `platform.twitter.com/embed/Tweet.html?id=ID`
   → `https://x.com/i/status/ID`, `x.com/u/status/ID` → kept. Run as a single
   `lol_html` element handler. Unblocks fixture 8 fully; partial unblock on
   any fixture with embedded media.

3. **GitHub chrome selectors** (helps gap #7, ~20 LOC of selector strings):
   add `react-partial, tool-tip, .progress-pjax-loader, .js-stale-session-flash-*,
   .Skip-to-content, link[rel="stylesheet"]` to `constants.rs::REMOVAL_SELECTORS`.
   Won't make GitHub perfect, but cuts the 65 KB output to under 10 KB and
   removes the bizarre "Constants loaded at…" stray text.

4. **Wikipedia infobox + chrome selectors** (helps gap #8, ~15 LOC of selectors):
   `.infobox, .navbox, .reference, .mw-editsection, #mw-navigation, #footer,
   .mw-jump-link, #toc` in removal selectors. Doesn't handle citation
   rewriting (that needs the full extractor) but immediately drops the body
   noise.

5. **Generic `<svg>` removal** (helps gaps #7, #10, ~5 LOC): unconditional
   `svg` removal selector. Trek currently leaves all SVGs — the Obsidian
   callout fixture (#7) emits an inline 24×24 lucide-info SVG before every
   callout. The Reddit/HN fixtures don't show SVGs but real-world Reddit /
   GitHub do.

6. **Empty wrapper-div flattening for Substack-style class soup**
   (helps gap #6, ~30 LOC): an iterative pass that collapses `<div>` nodes
   whose only contribution is layout classes (`pencraft`, `pc-…`, `flex-…`).
   `standardize.rs` already has wrapper flattening
   (`test_flatten_wrapper_divs` at `src/standardize.rs`); extending the
   class allowlist could substantially reduce Substack output.

7. **Add `og:site_name` → `metadata.site` fallback** (helps gap #5, ~10 LOC):
   the Wikipedia fixture has the meta tag but `metadata.site` comes back
   `""`. Quick check of `MetadataExtractor::extract_from_collected_data` will
   show whether the property name list is missing `og:site_name` (most
   likely) or the precedence is wrong.

A combined pass on quick-wins 1–7 is plausibly 200 LOC and would move 4 of
the 8 fixtures' metadata to fully-OK. Body content still depends on gap #1
(markdown) and #2/#3 (extractors).

## 6. Suggested track sequencing implication

Without prescribing the refactor design, the data above suggests:

1. **Track A (markdown converter)** is the single highest-leverage piece;
   blocks 100% of fixture parity.
2. **Track B (registry wiring + 4 priority site extractors:
   reddit / hackernews / substack / github)** unlocks the
   "trash-output" cluster. Wikipedia can be small (24 LOC) once the registry
   exists.
3. **Track C (code-blocks + callouts + embeds + footnotes elements)**
   parallels Track B; both depend on the markdown converter from Track A but
   not on each other.
4. **Quick-wins** can land independently inside Track A as warm-up PRs —
   they don't need refactor architecture decisions.

## 7. Tooling produced

- `examples/extract_file.rs` — minimal Trek CLI: `cargo run --example
  extract_file -- <path.html> [url]` prints a JSON summary including the
  first 2 KB of `content` and the (current always-null) `content_markdown`.
- `scripts/refactor/compare-fixture.sh` — runs Trek over one or more
  Defuddle fixtures and dumps a side-by-side diff under
  `/tmp/trek-gap-results/`. `--all` runs the canonical 8-fixture sample used
  in this doc, so future agents can reproduce the table in §3 with one
  command.

Both are plumbing, not production code. The example file is at the standard
Cargo location and ships with the repo automatically; the shell script is
under `scripts/refactor/` so it doesn't pollute the main `scripts/` tree.

## 8. Reference: Defuddle module sizes for budget planning

| Defuddle file | LOC | Trek equivalent | LOC |
|---|---|---|---|
| `src/markdown.ts` | 813 | (none) | 0 |
| `src/elements/footnotes.ts` | 1,235 | `src/elements/footnotes.rs` | (small, see existing) |
| `src/elements/code.ts` | 514 | `src/elements/code.rs` | (small) |
| `src/elements/callouts.ts` | 138 | (none) | 0 |
| `src/elements/headings.ts` | — | `src/elements/headings.rs` | exists |
| `src/elements/images.ts` | — | `src/elements/images.rs` | exists |
| `src/extractors/*.ts` | 6,576 (25 files) | `src/extractors/mod.rs` | stub only |
| `src/standardize.ts` | — | `src/standardize.rs` | 274 |
| `src/metadata.ts` | — | `src/metadata.rs` | 314 |
| `src/removals/*.ts` (6 files) | — | merged into `constants.rs` | 522 |

Total Trek source: 2,126 LOC across the modules listed (counted via `wc -l`
on the files in §1's reference), versus a Defuddle equivalent that's roughly
3–4× larger. The expected end state of the refactor is roughly Trek = 5–7
KLOC of Rust, comfortably staying under the 3 MB compressed Worker limit
even if the WASM grows ~30% from added modules.
