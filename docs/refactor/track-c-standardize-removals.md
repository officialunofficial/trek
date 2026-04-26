# Track C — Port Defuddle's `standardize` + `removals` Modules to Trek

Status: design spec. No production code in this document.
Owner: Trek extraction-quality team.
Source of truth (TS): `/tmp/defuddle-clone/src/standardize.ts` + `/tmp/defuddle-clone/src/removals/*.ts` + `/tmp/defuddle-clone/src/content-boundary.ts`.
Target (Rust): `/Users/christopherw/Workspaces/officialunofficial/trek/src/standardize.rs` (274 lines, ~10% feature-parity) + `src/scoring.rs` (text-only proxy) + `src/lib.rs::remove_clutter` (regex/lol\_html toy).

The accuracy gap to Defuddle is dominated by two clusters: (1) the standardize pipeline that turns "raw extracted DOM" into "normalized prose," and (2) the removal heuristics that excise clutter the selector pass missed. Both clusters are written against a real DOM. Trek's lol\_html streaming approach cannot express most of them.

---

## 1. Inventory — `standardize.ts` transformations

`standardize.ts` exposes `standardizeContent(element, metadata, doc, debug, subProfile)` (line 155) which orchestrates ~25 sub-passes. Each row maps a Defuddle pass to Trek's current state.

Legend: **F** = full parity; **P** = partial / regex-only; **N** = none.

| # | Pass (TS fn) | One-line description | Trek |
|---|---|---|---|
| 1 | `standardizeDropCaps` (574) | Merge `<span data-caps="initial">T</span><small>HE…</small>` into `THE…` | N |
| 2 | `standardizeSpaces` (254) | Replace `\xA0` with space in text nodes (skips pre/code/SVG) | P (regex over whole HTML, no pre/code skip) |
| 3 | `removeHtmlComments` (416) | TreeWalker removal of all comment nodes | P (regex `<!--…-->` — non-greedy, fragile) |
| 4 | `standardizeHeadings` (384) | Demote every `<h1>` to `<h2>`; remove first `<h2>` if equals title (minus permalink anchors) | N (no-op stub) |
| 5 | `wrapPreformattedCode` (237) | Wrap any `<code style="white-space:pre">` in a `<pre>` | N |
| 6 | `standardizeElements` (1225) | Apply `ELEMENT_STANDARDIZATION_RULES` (math, code-block, headings, images, role-based div→p / div→ul/li); arXiv LaTeXML cleanup; lite-youtube → iframe; remove empty tables; unwrap single-column tables; merge adjacent Verso code blocks; add `controls` to `<video>` | N |
| 7 | `resolveSvgColors` (684) | Resolve `var(--…)` / `light-dark()` in SVG fill/stroke/style; map Tailwind tokens to hex; apply fallback fill/stroke per tag class | N |
| 8 | `applySvgFallbackStyles` (783) | Add explicit fill/stroke for SVG path/rect/circle/ellipse/polygon/line/polyline/text when class hints styling | N |
| 9 | `resolveTailwindClasses` (842) | Map `fill-amber-600` / `stroke-current` etc. to inline style | N |
| 10 | `replaceCustomElements` (547) | Replace hyphenated custom elements (e.g. `<my-widget>`) with `<div>` (skips inline & SVG) | N |
| 11 | `convertDataAsSpans` (605) | Convert `<span data-as="p|h1…|li|blockquote">` into the named tag | N |
| 12 | `convertBlockSpans` (627) | `<span class="block …">` or inline `display:block` → `<p>` | N |
| 13 | `unwrapLayoutTables` (510) | Unwrap `<table>` whose only non-empty cell holds a single block element | N |
| 14 | `flattenWrapperElements[1]` (1520) | Multi-pass: drop wrapper `<div>`s lacking inline content; convert inline-only blocks to `<p>`; collapse single-block-child wrappers; merge deeply-nested wrappers | P (one-pass, lol\_html, only inspects `class`/`role`/`aria-label`) |
| 15 | `removePermalinkAnchors` (in `elements/headings`) | Remove `<a>` permalink markers (`¶`, `#`, `§`) inside headings | N |
| 16 | `stripUnwantedAttributes` (434) | Walk all elements; keep `ALLOWED_ATTRIBUTES` set; preserve `id` for footnotes, `class` for `language-*`/`callout`/`footnote-backref`; strip `class` on SVG | N (no-op stub) |
| 17 | `unwrapBareSpans` (646) | Unwrap any `<span>` with zero attributes (deepest-first) and `element.normalize()` | N |
| 18 | `unwrapSpecialLinks` (185) | Unwrap `<a>` inside `<code>`; `javascript:` links; restructure heading-wrapping anchors `<a><h2>X</h2></a>` → `<h2><a>X</a></h2>`; unwrap `a[href^=#]` containing a heading | N |
| 19 | `removeObsoleteElements` (217) | Remove `object, embed, applet` | N (relies on selector list) |
| 20 | `removeEmptyElements` (904) | Deepest-first removal of elements with no text/`\xA0`; treats `<br>` as empty; comma-only div detection; honours `ALLOWED_EMPTY_ELEMENTS` | P (regex `<p></p>`/`<div></div>` only) |
| 21 | `removeTrailingHeadings` (284) | Walk headings bottom-up, remove if no content after (recurses through parent siblings) | N |
| 22 | `removeOrphanedDividers` (337) | Strip leading/trailing `<hr>`; collapse consecutive `<hr>` | N |
| 23 | `flattenWrapperElements[2]` + `removeOrphanedDividers[2]` | Re-runs after empty removal — second-order cleanup | N |
| 24 | `stripExtraBrElements` (955) | Keep ≤2 consecutive `<br>`; remove `<br>` between block siblings; remove trailing `<br>` inside blocks | N |
| 25 | `removeEmptyLines` (1086) | Two-pass: collapse `\n\r\t`/zero-width chars in text nodes; trim block edges; move whitespace outside inline elements; insert space between adjacent inline siblings | P (whole-HTML space collapse — destroys `<pre>` content; current Trek test acknowledges this) |
| 26 | `mergeAdjacentVersoCodeBlocks` (1436) | Merge consecutive `<pre data-verso-code="true">` blocks of same language | N |

**Math / Code / Heading / Image element rules** (referenced from `ELEMENT_STANDARDIZATION_RULES`):
- `mathRules` (`elements/math.ts`): KaTeX/MathJax detection, MathML cleanup, `<math data-latex>` wrappers.
- `codeBlockRules` (`elements/code.ts`): unify highlight.js/Prism/Shiki/CodeMirror into `<pre><code class="language-X">`.
- `headingRules` (`elements/headings.ts`): permalink anchor handling, GitHub `markdown-heading` wrappers.
- `imageRules` (`elements/images.ts`): `<picture>` flattening, lazy-src promotion, `data-src` → `src`, base64 placeholder detection, srcset selection.

Trek currently has `src/elements/` (skeleton) but none of these rules are populated.

**Net:** Trek implements ~3 of ~26 standardize passes, all via brittle regex.

---

## 2. Inventory — `removals/*` modules (6 files, 2364 LOC)

### 2.1 `removals/selectors.ts` (179 LOC) — `removeBySelector`
- Two-stage match: exact selector list (`EXACT_SELECTORS_JOINED`) + partial-attribute regex (`PARTIAL_SELECTORS_REGEX`) checked against `class`, `id`, `data-component`, `data-test(id)?`, `data-qa`, `data-cy`.
- Skip rules: inside `pre`/`code`; SVG `<style>`; `data-defuddle` subtrees; ancestors of `mainContent`; footnote lists & their immediate children/parents; `a` inside headings; responsive show classes (`hidden sm:flex`).
- Post-rules: `<button>` containing media → keep media, drop button; `<button>` inside inline → unwrap.
- **Trek:** has the constant lists. `lib.rs::remove_clutter` does a coarse approximation — checks 7 hard-coded class names exactly, no `id`/`data-*` attribute matching beyond `class`, no responsive guards, no footnote protection, no button-media salvage.

### 2.2 `removals/hidden.ts` (90 LOC) — `removeHiddenElements`
- Inline `style` regex for `display:none|visibility:hidden|opacity:0`.
- `getComputedStyle` only when running in a real browser (skipped under jsdom/linkedom).
- Class-based: `hidden`, `invisible`, prefix variants (`sm:hidden`, `lg:invisible`); excludes Tailwind arbitrary variants `[&_.x]:hidden`.
- Preserves elements containing `<math>`, `[data-mathml]`, `.katex-mathml` (Wikipedia accessibility pattern).
- **Trek:** none.

### 2.3 `removals/metadata-block.ts` (34 LOC) — `removeMetadataBlock`
- Removes a date-bearing sibling within 3 elements of the content `<h1>`. Only fires when metadata extraction has produced an author or published date (gated by caller).
- **Trek:** none.

### 2.4 `removals/small-images.ts` (166 LOC) — `findSmallImages` / `removeSmallImages`
- Min dimension = 33 px. Sources: `width`/`height` attrs, inline style, computed style/bounding rect (browser only), SVG viewBox, srcset 1x URL hint (`?w=32`).
- Skip math/LaTeX images by alt or class.
- Also removes broken images (no src + no fallback) and base64 placeholders not under `<picture>`.
- **Trek:** image-size filtering exists only in the fallback first-image-from-content scan (50×50, hard-coded).

### 2.5 `removals/scoring.ts` (566 LOC) — `ContentScorer`
- `scoreElement` — positive: words, paragraph count × 10, commas, content classes (`content|article|post|entry`), date/author patterns, footnotes, table-cell layout heuristic, position bonus. Negative: image density, nested tables. Multiplier: link density up to 50% reduction.
- `scoreNonContentBlock` — penalty model used by `scoreAndRemove`: navigation indicator regexes (-10 each), link density >0.5 (-15), high link-text ratio (-15), social profile URLs (-15), byline-with-date heuristic (-10), card-grid detection (-15), classes matching `nonContentPatterns` (-8).
- `isLikelyContent` — guard before scoring: roles, content indicator words, contains `pre|table|figure|picture`, heading wrappers, navigation-heading detection, card-grid bailout, prose-with-low-link-density rule.
- `isCardGrid` — block with 3+ headings & 2+ images & <20 prose words per heading.
- **Trek:** `src/scoring.rs` operates on **string content** (regex over `<p>`, `<a>`, `<img>`). It cannot identify a tree element to remove and is never wired into a tree-mutation pass.

### 2.6 `removals/content-patterns.ts` (1229 LOC) — `removeByContentPattern` (+ helpers)
This is the heaviest module. Each bullet is one independent heuristic with its own DOM walk:
1. Breadcrumb list at start.
2. Promotional banner `<a>` (block `<a>` before first `<h1>`).
3. Hero header (container wrapping `h1` + `time` + author + tags + image).
4. "Listen to this article" audio widgets + pre-content audio/video in short containers.
5. Table of contents (3+ same-page anchor links near top, with surrounding ToC heading + framing `<hr>`).
6. Timezone widget ("Current time in"), pinned labels.
7. Pre-content duplicate of title or description.
8. Article metadata header blocks (date / "21 hours ago - Politics") above content.
9. Category badges (image + tag link, <5 words).
10. "By [Name]" bylines near start.
11. Read-time metadata ("8 min read").
12. Author + date in same block near start.
13. Standalone date elements near start.
14. Standalone `<time>` near start or end (walk up inline-formatting wrappers only).
15. Blog metadata lists (`ul`/`ol`/`dl` of label-value pairs near boundaries).
16. Section breadcrumbs / back-navigation links via URL-path comparison (`linkPath` is prefix of current URL).
17. Trailing external link lists (heading + ul of off-site-only links as last block).
18. Trailing related-posts blocks (last container of all link-dense paragraphs).
19. Trailing thin sections (last children with heading but <15% of words & no content elements).
20. Boilerplate sentence patterns (10+ regex list, including ©, "originally appeared in", "Loading…", "Read our Comment Policy") with cascade-truncate.
21. Heading-text matching for "Related posts / Read next / About the Author / Comments" with cascade.
22. Orphaned "For more on/about…" intro paragraphs.
23. Card grids without detectable heading.
24. Newsletter signup containers (text-pattern detector incl. camelCase normalization).
25. Newsletter signup `<ul>`s.
26. Author/contact info blocks near end (label + email/phone/mailto).
27. Author/share metadata widgets ("Share", "Follow", "Authors").
28. Social engagement counters ("9 Likes", "3 Comments").
29. Trailing tag/category link blocks.

Helpers: `removeTrailingSiblings`, `removeTrailingWithCascade`, `walkUpToWrapper`, `walkUpIsolated`, `removeThinPrecedingSection`, `isNewsletterElement`, `isBreadcrumbList`, `removeHeroHeader`, `removeEyebrowLabel`, `findContentStart` (from `content-boundary.ts`).

**Trek:** none.

### 2.7 `content-boundary.ts` (144 LOC)
- `findContentStart(mainContent, title)` — locates the prose boundary used by 9+ patterns above. Anchors on the title `<h1>`/`<h2>`, then walks forward in document order via `TreeWalker` for the first prose-block (P / DIV / SECTION / ARTICLE / BLOCKQUOTE / FONT) with ≥7 words, sentence punctuation, low link density, no dialog/script/style descendants.
- `isAboveContentStart(el, boundary)` — `compareDocumentPosition` wrapper.
- **Trek:** none. The prose-boundary primitive is required by Track C — many removals are scoped to "above content start."

---

## 3. Architectural recommendation — DOM library

### 3.1 The constraint

Of the 26 standardize passes and 35+ removal heuristics inventoried above:
- **~52 of ~61** require multi-pass parent/sibling/`compareDocumentPosition` access (deepest-first iteration, sibling cascades, walk-up, document-order comparison).
- **~9** can be expressed as single-pass attribute mutations (which lol\_html handles).

lol\_html is a single-pass streaming rewriter. It does not expose siblings, depth, or document-order comparison. Continuing on lol\_html means re-implementing a tree on the side (build → mutate → serialize), which is exactly what kuchikiki/scraper already do — but worse, because we'd be hand-rolling.

### 3.2 Survey

| Crate | Backend | DOM API surface | WASM-friendly? | Approx WASM cost (gz) | Notes |
|---|---|---|---|---|---|
| `html5ever` | Mozilla Servo parser | tokenizer + tree builder (raw) | yes (no syscalls) | ~120 KB | Foundation only — you build your own DOM on top. |
| `markup5ever` | shared types for html5ever / xml5ever | strings/atoms/namespaces | yes | shared | Indirect dep of `kuchikiki` & `scraper`. |
| `kuchiki` | html5ever + RcDom | full DOM, CSS selectors, parent/sibling refs, mutation | yes | ~180 KB | **Unmaintained** since 2021. Last release 0.8.1. |
| `kuchikiki` | html5ever + RcDom | drop-in fork of `kuchiki`, actively maintained (Servo team) | yes | ~180 KB | Re-export of `kuchiki` API. Used by `readability` Rust ports. |
| `scraper` | html5ever + ego-tree | read-mostly DOM, CSS selectors via `selectors` crate | yes (used by spiders) | ~250 KB | Mutation requires rebuilding the tree — wrong shape for our use. |
| `lol_html` (current) | streaming | element callbacks only | yes | ~80 KB | What we have. Cannot do siblings/parents. |

### 3.3 Recommendation: **add a `kuchikiki` pass after lol\_html (option b)**

Keep lol\_html for what it does well — initial pass, metadata collection, body-content extraction, attribute streaming, the cheap selector-based removals — and add a kuchikiki tree-mutation pass for standardize + content-pattern removals + scoring.

Why kuchikiki and not the alternatives:
- `scraper`: mutation API is read-mostly; we need element replacement, fragment splicing, node moves. Wrong abstraction.
- `kuchiki`: same API, unmaintained.
- `html5ever` raw: writing our own DOM is months of work and reinvents kuchikiki.
- Replacing lol\_html entirely: throws away a fast streaming first-pass that already works for metadata. Roundtrip cost goes up for sites that don't need standardize (pre-extracted Markdown, etc.).

Pipeline becomes:
1. lol\_html pass — collect metadata, schema.org, title, favicon, meta tags. (Unchanged.)
2. lol\_html pass — `extract_body_content` + cheap exact-tag selector removal (`script, style, nav, …`). (Unchanged.)
3. **NEW kuchikiki parse** — produces an in-memory tree.
4. Selector-removal phase against the tree (port of `removals/selectors.ts`).
5. Hidden-element removal (port of `removals/hidden.ts`).
6. Small-image removal (port of `removals/small-images.ts`).
7. Score-and-remove (port of `removals/scoring.ts::ContentScorer.scoreAndRemove`).
8. `findContentStart` once → cached.
9. `removeByContentPattern` (port of `removals/content-patterns.ts`).
10. `removeMetadataBlock` (gated on metadata having author or date).
11. **NEW standardize pipeline** in tree form (the 26 passes from §1).
12. Serialize tree back to HTML.

WASM size impact: +~190 KB compressed (~600 KB raw) for kuchikiki + html5ever + selectors transitive. With `opt-level = "z"` and `lto = "fat"` already set in `Cargo.toml`, projection: current WASM ~180 KB → ~360–400 KB. Mitigation: see §8.

### 3.4 What can stay on lol\_html

Some passes are genuinely streaming-friendly and cheaper there:
- `removeHtmlComments` (just drop comment nodes during parse — `lol_html::doc_comments`).
- `removeObsoleteElements` (`object, embed, applet` exact tag removal).
- The cheap exact-selector removal of `script, style, nav, footer, header, aside, noscript`.
- Initial body-extraction.

Keep these on lol\_html. Everything tree-shaped goes to kuchikiki.

---

## 4. Module layout to add

```
src/
  content_boundary.rs            # findContentStart / isAboveContentStart
  dom/
    mod.rs                       # parse(html) -> NodeRef, serialize(NodeRef) -> String
    walk.rs                      # treewalk helpers, deepest-first iter, normalize()
    selectors.rs                 # cached CSS selector compilation
    text.rs                      # text_content(), normalize_text(), count_words()
  standardize/
    mod.rs                       # public standardize_content(root, metadata, debug)
    drop_caps.rs                 # pass 1
    spaces.rs                    # pass 2
    comments.rs                  # pass 3 (or stay in lol_html pre-pass)
    headings.rs                  # pass 4 + permalink anchor removal
    pre_code.rs                  # pass 5 wrapPreformattedCode
    elements.rs                  # pass 6 standardizeElements + ELEMENT_STANDARDIZATION_RULES
    svg_colors.rs                # passes 7-9
    custom_elements.rs           # pass 10
    data_as_spans.rs             # pass 11
    block_spans.rs               # pass 12
    layout_tables.rs             # pass 13
    flatten_wrappers.rs          # pass 14, 23
    attributes.rs                # pass 16
    bare_spans.rs                # pass 17
    special_links.rs             # pass 18
    empty_elements.rs            # pass 20
    trailing_headings.rs         # pass 21
    orphan_dividers.rs           # pass 22
    br_elements.rs               # pass 24
    empty_lines.rs               # pass 25
    verso.rs                     # pass 26
    rules/
      math.rs                    # mathRules
      code.rs                    # codeBlockRules
      heading_rules.rs           # headingRules + isPermalinkAnchor
      images.rs                  # imageRules + isBase64Placeholder
  removals/
    mod.rs                       # public run_removals(root, opts, metadata, debug)
    selectors.rs                 # removeBySelector
    hidden.rs                    # removeHiddenElements
    metadata_block.rs            # removeMetadataBlock
    small_images.rs              # findSmallImages + removeSmallImages
    scoring.rs                   # ContentScorer (renamed; current scoring.rs becomes legacy/ removed)
    content_patterns.rs          # removeByContentPattern + helpers
    helpers.rs                   # walk_up_to_wrapper, walk_up_isolated, remove_trailing_*, etc.
```

Existing `src/scoring.rs` is replaced by `src/removals/scoring.rs`. Existing `src/standardize.rs` is split into `src/standardize/` and deleted.

---

## 5. Migration plan — port order

Order by "blast radius on fixture diffs," front-loading the changes that unblock other passes.

**Phase 1 — DOM substrate** (blocks everything)
1. `src/dom/mod.rs` parse/serialize via kuchikiki.
2. `src/dom/walk.rs` treewalker, deepest-first iterator, `normalize()` (merge adjacent text nodes), text-content helpers.
3. `src/dom/selectors.rs` cached `Selectors` instances for the hot selectors.
4. Wire `Trek::parse_internal` to: lol\_html pre-pass → kuchikiki parse → kuchikiki passes → serialize.

**Phase 2 — Removals that already have selector lists** (largest immediate fixture win)
5. Port `removals/selectors.rs` — Trek already has `EXACT_SELECTORS`, `PARTIAL_SELECTORS`, `TEST_ATTRIBUTES`, `PARTIAL_SELECTORS_REGEX`, `FOOTNOTE_*`. Just needs DOM mutation.
6. Port `removals/hidden.rs` — small, self-contained.
7. Port `removals/small-images.rs`.

**Phase 3 — Standardize core** (the visual-quality gap)
8. `attributes.rs` (`stripUnwantedAttributes`) — required by every downstream pass that compares classes.
9. `flatten_wrappers.rs` — biggest visible structural win; current Trek implementation is a one-pass stub.
10. `bare_spans.rs` + `empty_elements.rs` + `orphan_dividers.rs` + `br_elements.rs` — cluster of "remove cruft" passes that compound.
11. `headings.rs` (h1→h2 + title-h2 dedupe) — required for `findContentStart`.
12. `special_links.rs` — fixes heading-wrapping anchors; many fixtures regress without it.
13. `pre_code.rs`, `custom_elements.rs`, `data_as_spans.rs`, `block_spans.rs`, `layout_tables.rs`.
14. `empty_lines.rs` — last; depends on most others.

**Phase 4 — Content boundary + scoring**
15. `content_boundary.rs::find_content_start` — required by Phase 5.
16. `removals/scoring.rs` — `ContentScorer::score_and_remove`.

**Phase 5 — Content-pattern removals**
17. `removals/content_patterns.rs` — port one heuristic at a time, in the order they appear in the TS file (pattern order matters: ToC removal before metadata removal, etc.). Each heuristic is 20–80 LOC and gets its own test.
18. `removals/metadata_block.rs`.

**Phase 6 — Element rules**
19. `standardize/rules/heading_rules.rs` (permalink anchors).
20. `standardize/rules/code.rs` (highlighter unification).
21. `standardize/rules/images.rs` (`<picture>` flattening, lazy-src).
22. `standardize/rules/math.rs` (lowest priority — niche).
23. SVG color resolution (passes 7–9).
24. `verso.rs` (lowest priority).

Fixture tests that will fail at the start of Phase 1 and turn green progressively:
- Phase 2: any fixture with `data-testid` clutter, hidden elements, tracking pixels.
- Phase 3: every fixture with deeply nested wrapper divs, bare spans, attribute noise.
- Phase 5: every news/blog fixture with bylines, dates, ToCs, related-posts blocks, newsletter signups.

---

## 6. Concrete file list

New files to create (paths absolute-relative to repo root):

| Path | Purpose |
|---|---|
| `src/dom/mod.rs` | kuchikiki parse/serialize wrappers; `Document` newtype |
| `src/dom/walk.rs` | Treewalker, deepest-first iter, `normalize_text_nodes` |
| `src/dom/selectors.rs` | Lazy compiled `Selectors` for hot CSS selectors |
| `src/dom/text.rs` | `text_content`, `normalize_text`, `count_words`, `link_text_length` |
| `src/content_boundary.rs` | `find_content_start` + `is_above_content_start` |
| `src/standardize/mod.rs` | Re-exports + `standardize_content` orchestrator |
| `src/standardize/{drop_caps,spaces,comments,headings,pre_code,elements,svg_colors,custom_elements,data_as_spans,block_spans,layout_tables,flatten_wrappers,attributes,bare_spans,special_links,empty_elements,trailing_headings,orphan_dividers,br_elements,empty_lines,verso}.rs` | One file per pass (21 files) |
| `src/standardize/rules/{math,code,heading_rules,images}.rs` | Element-rule sets (4 files) |
| `src/removals/mod.rs` | Re-exports + `run_removals` orchestrator |
| `src/removals/selectors.rs` | `remove_by_selector` |
| `src/removals/hidden.rs` | `remove_hidden_elements` |
| `src/removals/metadata_block.rs` | `remove_metadata_block` |
| `src/removals/small_images.rs` | `find_small_images` + `remove_small_images` |
| `src/removals/scoring.rs` | `ContentScorer` (replaces `src/scoring.rs`) |
| `src/removals/content_patterns.rs` | `remove_by_content_pattern` (the 29 heuristics) |
| `src/removals/helpers.rs` | `walk_up_to_wrapper`, `walk_up_isolated`, `remove_trailing_siblings`, `remove_trailing_with_cascade`, `remove_thin_preceding_section`, `is_breadcrumb_list`, `is_newsletter_element`, `is_or_contains_heading` |
| `src/constants.rs` | Add: `BLOCK_LEVEL_ELEMENTS`, `BLOCK_ELEMENTS_SELECTOR`, `EXACT_SELECTORS_JOINED`, `HIDDEN_EXACT_SELECTOR`, `HIDDEN_EXACT_SKIP_SELECTOR`, `TEST_ATTRIBUTES_SELECTOR`, `CONTENT_ELEMENT_SELECTOR`, `TAILWIND_COLORS`, `TAILWIND_SPECIAL`, `ALLOWED_ATTRIBUTES_DEBUG` |
| `tests/fixtures_standardize.rs` | Per-pass test corpus |
| `tests/fixtures_removals.rs` | Per-heuristic test corpus |
| `docs/refactor/track-c-standardize-removals.md` | This document |

Files to delete after migration: `src/standardize.rs`, `src/scoring.rs`.

---

## 7. `Cargo.toml` additions

```toml
[dependencies]
# Real-DOM pass (post-lol_html)
kuchikiki = "0.8"          # html5ever-backed DOM with parent/sibling refs
html5ever = "0.27"          # transitively required by kuchikiki; pin to silence dep-version warnings
selectors = "0.25"          # already a transitive dep via kuchikiki
markup5ever = "0.12"        # ditto
# (Optional, for treewalker convenience — re-evaluate if not needed)
ego-tree = { version = "0.6", optional = true }
```

No new wasm-bindgen surface required. `getrandom = { features = ["js"] }` is already set.

Lint config: `kuchikiki` triggers `clippy::multiple_crate_versions` (already allowed) — no new clippy allows needed.

---

## 8. WASM size impact + mitigation

**Projection (gzipped):**
- Current Trek WASM (release, opt-level=z, lto, strip): ~180 KB.
- Kuchikiki + html5ever + selectors: +~190 KB.
- New Rust code (~3500 LOC standardize + ~1500 LOC removals): +~30–50 KB after `opt-level=z`.
- **Estimated final: 380–420 KB gzipped.**

For comparison, Defuddle's minified ESM bundle is ~280 KB gzipped (without metadata extractors). Trek shipping at ~400 KB for parity is acceptable for the use cases (server-side WASM, native binary, browser extensions where 400 KB is below the 500 KB rule-of-thumb).

**Mitigations:**
1. **Feature gate the heavy DOM pass.** Add `default-features = ["dom"]` and a `dom` feature that gates kuchikiki + standardize + removals. Embedded users who want metadata-only get the small bundle.
2. **Drop `regex` from hot paths inside the kuchikiki pass.** `html5ever` already brings its own ASCII routines. Replace `regex` for fixed-string searches with `memchr`/`bytes`. (Saves ~80 KB by allowing `regex` to be tree-shaken if no other module uses it. Currently several modules use it, so audit first.)
3. **Compile selectors lazily once, in `OnceLock`/`once_cell`.** Already in `Cargo.toml` via `once_cell`. Each compiled `Selectors` adds ~1 KB; we want to share them.
4. **Avoid `tracing-subscriber` on wasm32.** Already gated. Keep that.
5. **Strip in release; the existing `strip = true` already runs.**
6. **Run `wasm-opt -Oz`** as a post-build step in `make wasm-build`. Confirm not already running. Easy ~10 % saving.
7. **Drop `color-eyre`** (currently in deps) on `wasm32` — pretty-printing is dead code in WASM. Saves ~30 KB.
8. **Audit `serde_json`** features — if only used to parse schema.org, switch to `raw_value` + manual extraction; saves ~40 KB. Probably out of scope for Track C.

Acceptance criterion for size: ≤450 KB gzipped after all of Phase 1–5 lands. If exceeded, gate Phase 6 (rules) behind a feature flag.

---

## Appendix A — Open questions for implementer

1. Some Defuddle helpers depend on `Element.compareDocumentPosition`. kuchikiki exposes node ordering via `NodeRef::following` iteration but not a direct bitmask compare. Either implement `compare_document_position` ourselves (walk-up-to-LCA + index-of) or replace usages with index-based comparison. Recommendation: implement the helper once in `src/dom/walk.rs`, model the bitmask result as an enum.
2. `getComputedStyle` is browser-only. Defuddle gates it on `typeof window !== 'undefined'`. We are always in a non-browser context (WASM running in Node/Cloudflare/Bun, or native Rust). All `isBrowser` branches should be ported as `false` — drop the conditional code entirely.
3. The `isLikelyContent` heuristic (`removals/scoring.ts:321`) protects content from removal *and* protects content from re-flattening. Decide whether to share this predicate between standardize/flatten and removals/scoring — recommended: yes, expose from `src/removals/scoring.rs` and import in `standardize/flatten_wrappers.rs`.
4. Verso (Lean docs) merging is niche; gate behind a feature flag if it bloats binary.
5. Tailwind color resolution table (`TAILWIND_COLORS`) is ~1100 lines of hex constants. Consider generating it via `build.rs` from a JSON file rather than maintaining a giant Rust literal.
