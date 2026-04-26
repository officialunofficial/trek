# Track C — Unimplemented standardize passes

Phase-1 lands the highest-impact subset of Defuddle's `standardize.ts`. The
following passes still need to be ported. Each row tracks `defuddle/src/standardize.ts`
function name, line range, and notes.

| Pass | Status | Notes |
|---|---|---|
| `standardizeDropCaps` | TODO | Niche; affects ~3 fixtures. |
| `standardizeSpaces` (DOM-aware) | PARTIAL | Currently in `standardize_legacy.rs`; preserves `<pre>`. Migrate to DOM pass that walks text nodes and respects pre/code/SVG. |
| `removeHtmlComments` (DOM) | PARTIAL | `standardize_legacy.rs` handles via regex. |
| `wrapPreformattedCode` | PARTIAL | Implemented inside `code_blocks.rs::Pass A`. |
| `standardizeElements` (full ELEMENT_STANDARDIZATION_RULES) | TODO | Math, complex code-block, image, video chrome rules. |
| `resolveSvgColors` | TODO | SVG fill/stroke `var(--…)` resolution. |
| `applySvgFallbackStyles` | TODO | |
| `resolveTailwindClasses` | TODO | Massive token table. |
| `replaceCustomElements` | TODO | Hyphenated custom elements → `<div>`. |
| `convertDataAsSpans` | DONE | `promote_semantics.rs` |
| `convertBlockSpans` | DONE | `promote_semantics.rs` |
| `unwrapLayoutTables` | DONE | `tables.rs` |
| `flattenWrapperElements` | DONE | `flatten_wrappers.rs` |
| `removePermalinkAnchors` | PARTIAL | `headings.rs` strips empty/¶/§ anchors. |
| `stripUnwantedAttributes` | TODO | `standardize_legacy.rs` is a stub. |
| `unwrapBareSpans` | DONE | `promote_semantics.rs` |
| `unwrapSpecialLinks` | PARTIAL | `promote_semantics.rs` handles js: + heading-wrapping anchors. |
| `removeObsoleteElements` (object/embed/applet) | TODO | Currently relies on lol_html selector list. |
| `removeEmptyElements` (DOM, deepest-first) | PARTIAL | `standardize_legacy.rs` regex covers `<p></p>`/`<div></div>`. |
| `removeTrailingHeadings` | DONE | `headings.rs` |
| `removeOrphanedDividers` | TODO | Strip leading/trailing `<hr>`. |
| `stripExtraBrElements` | TODO | Cap consecutive `<br>` at 2. |
| `removeEmptyLines` (DOM-aware) | TODO | Respect pre/code. |
| `mergeAdjacentVersoCodeBlocks` | TODO | Lean docs niche. |
| Element rules (math, images, headings, code) | PARTIAL | `code_blocks.rs` covers Chroma/Shiki/CodeMirror; image lazy-src lives in `figure_image.rs`. |

# Track C — Unimplemented removal heuristics

Of Defuddle's 29 content-pattern heuristics, the following are currently NOT
ported (see `removals/content_patterns.rs` for what is). Pick by fixture impact.

* Hero header (h1 + time + tags + image wrapper)
* "Listen to this article" audio widgets
* Table of contents at top
* Timezone widgets ("Current time in")
* Pre-content duplicate of title or description
* Article metadata header blocks ("21 hours ago - Politics")
* Category badges (image + tag link, <5 words)
* Author + date in same block near start
* Standalone `<time>` or date elements near start/end
* Blog metadata lists (label-value pairs)
* Section breadcrumbs / back-navigation links via URL-path comparison
* Trailing external link lists
* Trailing thin sections
* Boilerplate sentence patterns (©, "originally appeared in", "Loading…")
* Heading-text matching with cascade (some forms covered)
* "For more on/about…" intro paragraphs
* Card grids without detectable heading
* Newsletter signup `<ul>`s (only signup containers handled)
* Author/contact info blocks near end (label + email/phone)
* Trailing tag/category link blocks
