# Track B — HTML → Markdown Conversion

Status: spec only, no production code yet.
Goal: port Defuddle's `src/markdown.ts` (Turndown-based) to Rust so Trek can emit
markdown alongside (or in place of) cleaned HTML.

Reference upstream: `/tmp/defuddle-clone/src/markdown.ts` (813 lines, Turndown
plus ~17 custom rules).

---

## 1. Build vs. Buy

### Survey

| Crate | Direction | Pluggable rules | DOM model | Verdict |
|---|---|---|---|---|
| `html2md` (0.2.x) | HTML → MD | yes (`Handler` trait, per-tag) | `html5ever` RcDom | **Closest fit.** Has handlers for headings/lists/tables/anchors/images, supports custom handlers. Missing: callouts, MathML, footnote refs, srcset, fenced lang detection, KaTeX, figure+caption merge, complex tables (`colspan`/`rowspan` HTML passthrough), Obsidian embeds. All addable via `Handler` impls. |
| `htmd` (0.1.x) | HTML → MD | yes (`element_handler`) | `html5ever` RcDom | Younger, smaller surface, fewer extension points than `html2md`. Tables and code fences are decent but no footnote/callout/math support. Comparable porting effort to `html2md` with less existing coverage. |
| `markup5ever` / `html5ever` | HTML parser only | n/a | RcDom / tree builder | The substrate the above use. Using directly means writing every rule from scratch — significantly more work but maximum control. |
| `pulldown-cmark` | MD → HTML (wrong direction) | n/a | n/a | Not applicable; only useful as a round-trip validator in tests. |
| Trek's `lol_html` (current) | HTML rewriter, streaming | event-based | no tree | Streaming-only; cannot do lookbehind/parent inspection (e.g., "is this `<sup>` inside a footnote `<ol>`?", "does this `<figure>` contain a `<p>` outside `<figcaption>`?"). The existing `html_to_text.rs` shows the limit: it can't do nested list indentation correctly. **Wrong tool for markdown.** |

### Recommendation: **fork `html2md` patterns, build a thin in-tree module on top of `html5ever` + `markup5ever_rcdom`.**

Rationale:
- Defuddle's rules need parent/sibling/ancestor inspection (`closest('table')`, "is parent OL?", "previousSibling text last char"). A streaming rewriter (`lol_html`) cannot do this. We need a parsed tree.
- `html5ever` + `markup5ever_rcdom` are already transitive deps of `html2md` and any RcDom-based crate. Adding them is cheap.
- `html2md` as a direct dependency is workable but has a sticky API (handlers receive a `&Handle` and a string buffer; it's hard to recursively re-enter the converter for nested structures like callouts and figures, both of which Defuddle does via `turndownService.turndown(innerHTML)` calls). Adopting its handler shape but owning the recursion is cleaner.
- WASM: `html5ever` and `markup5ever_rcdom` both compile to `wasm32-unknown-unknown`. Verified by their use in other published WASM crates (`html2md` itself ships a WASM build).
- Owning the module keeps clippy pedantic/nursery happy (third-party crates often lint-trip our config).

What we lose by not depending on `html2md` directly: ~300 lines of generic block/inline handlers we'll re-implement. Acceptable cost for control over edge cases.

What we lose by not writing on `lol_html` only: streaming. Acceptable: markdown conversion runs on already-extracted, already-cleaned content, which is small (typically <200 KB).

### Crates to add to `Cargo.toml`

```toml
html5ever = "0.27"
markup5ever_rcdom = "0.3"
# regex and once_cell are already deps
```

No new MathML→LaTeX crate. Defuddle falls back to `mathml-to-latex` (JS) only when no `data-latex` / `alttext` is present; we will mirror that fallback as "emit raw MathML inline" with a TODO. Adding a Rust MathML→LaTeX is a separate track.

---

## 2. Module Design — `src/markdown.rs`

### Public surface

```rust
/// Options controlling markdown rendering. Mirrors Defuddle's TurndownService config.
#[derive(Debug, Clone)]
pub struct MarkdownOptions {
    pub heading_style: HeadingStyle,    // Atx (default) | Setext
    pub bullet_marker: char,            // '-' (default), '*', '+'
    pub code_block_style: CodeBlockStyle, // Fenced (default) | Indented
    pub em_delimiter: char,             // '*' (default) or '_'
    pub hr: &'static str,               // "---"
    pub strip_leading_h1: bool,         // true: drop the article title if it's the first H1
    pub base_url: Option<String>,       // for resolving relative <img src>, <a href>
}

impl Default for MarkdownOptions { /* matches Defuddle defaults from markdown.ts:81-88 */ }

/// Convert a fragment of cleaned HTML to GitHub/Obsidian-flavoured markdown.
pub fn html_to_markdown(html: &str, opts: &MarkdownOptions) -> String;
```

### Internal architecture

```text
html_to_markdown(html, opts)
  ├─ parse_fragment(html)          -> RcDom tree (html5ever)
  ├─ Renderer { opts, footnotes, list_stack, in_table, ... }
  │     state machine carried through the tree walk
  ├─ render_node(handle) -> String (recursive)
  │     dispatches to per-tag block_* / inline_* handlers
  └─ post_process(md)              -> cleanup pass (regex)
```

### Block-level handlers (one per tag/cluster)

- `block_heading` — h1–h6, ATX style; preserves inline children. Handles `<a>` wrapping a heading by emitting heading then a "[View original](href)" link (`complexLinkStructure` rule).
- `block_paragraph` — `<p>`; renders inline children, strips leading/trailing whitespace, separates with blank line.
- `block_blockquote` — `<blockquote>`; prefixes each rendered line with `> `.
- `block_callout` — `div.callout[data-callout]`; emits Obsidian `> [!type]<fold> <title>\n> ...`. Removes `.callout-title` before recursing into `.callout-content`.
- `block_list` — `<ul>`, `<ol>`; tracks nesting depth in `list_stack`, special case `ol.ltx_enumerate` (arXiv) to renumber.
- `block_list_item` — `<li>`; bullet or `N.` prefix, tab-indents continuation lines per nesting level, handles `input[type=checkbox]` task list items (`[x]` / `[ ]`).
- `block_pre_code` — `<pre>` containing `<code>`; emits fenced block, language detected from `code[data-lang]`, `code[data-language]`, `class="language-xxx"`, `pre[data-language]` (rule `preformattedCode`). Trims and escapes literal backticks.
- `block_table` — `<table>`; three branches:
  1. `.ltx_equation` / `.ltx_eqn_table` / `.numblk` → equation table, extract MathML → `$$ ... $$`.
  2. layout tables (single column, no nested tables, ≤1 cell per row) → unwrap and render cells inline.
  3. complex tables (any `colspan`/`rowspan`) → emit cleaned raw HTML through (markdown allows raw HTML).
  4. simple tables → pipe-syntax with header separator row, pipes in cells escaped.
- `block_figure` — `<figure>`; if it contains `<p>` outside `<figcaption>`, treat as content wrapper and recurse normally. Otherwise emit `![alt](best_src)\n\n<caption>\n\n`. Caption math is converted to `$...$`.
- `block_hr` — `<hr>` → `\n---\n`.
- `block_footnote_list` — `ol` whose parent has `id="footnotes"`; emits `[^id]: text` per `<li>`, strips backref arrows (`↩︎`) and nested backref `<sup>` whose text equals the id.
- `block_div` — `<div>`; transparent passthrough (render children).
- `block_unknown` — fallback for unknown block elements: render children and add a blank line.

### Inline-level handlers

- `inline_text` — text node; HTML-entity decode, escape markdown-meaningful chars (`*`, `_`, `[`, `]`, `\``, `\`, `~`, `<`, `>` in specific positions only) per Turndown's `escape` semantics.
- `inline_strong` — `<strong>`, `<b>` → `**text**`.
- `inline_em` — `<em>`, `<i>` → `*text*` (or `_text_`).
- `inline_code` — `<code>` (not inside `<pre>`) → `` `text` ``; doubles backticks if content contains them.
- `inline_anchor` — `<a href>` → `[text](href "title?")`. Skips backref links (`href*=#fnref` or `class*=footnote-backref`) — they emit empty.
- `inline_image` — `<img>` → `![alt](best_src "title?")`. `best_src` picks largest from `srcset` (port `getBestImageSrc`, lines 41–77 of markdown.ts).
- `inline_strikethrough` — `<del>`, `<s>`, `<strike>` → `~~text~~`.
- `inline_highlight` — `<mark>` → `==text==`.
- `inline_break` — `<br>` → `  \n` (two trailing spaces, hard break) inside paragraphs.
- `inline_sub` / `inline_sup` — kept as raw HTML (`<sub>` / `<sup>`) per Defuddle's `keep` list, EXCEPT a `<sup id="fnref:N">` becomes `[^N]` (footnote ref, rule `citations`).
- `inline_keep` — `<iframe>`, `<video>`, `<audio>`, `<svg>`, `<math>` (when not handled by block math) — emit as raw HTML.
- `inline_math` — `<math>`, `.mwe-math-element`, `.mwe-math-fallback-image-*`; LaTeX from `data-latex` → `alttext` → MathML inner text fallback. Block math → `\n$$\n...\n$$\n`, inline math → `$...$` with smart spacing (port lines 591–614).
- `inline_katex` — `.math` / `.katex` → LaTeX from `data-latex`, then `.katex-mathml annotation[encoding="application/x-tex"]`, then text. Display vs inline detected via class or inner `<math display="block">`.
- `inline_embed` — `<iframe src=youtube>` / `<iframe src=twitter>` → Obsidian `![](url)` syntax (rule `embedToMarkdown`).
- `inline_button` — `<button>` → render children only, no markup.
- `inline_strip` — `<style>`, `<script>` → empty string.

### Post-processing pass (after tree walk)

Order matters; mirror lines 762–795 of markdown.ts:

1. Remove `<wbr/>` tags from input HTML (pre-pass before parsing).
2. Optionally strip leading `# Title\n+` if `opts.strip_leading_h1`.
3. Remove empty links: `\n*(?<!!)\[\]\([^)]+\)\n*` → `""` (preserve images).
4. Insert space between `!` and image syntax: `!(?=!\[|\[!\[)` → `! ` (prevents `Yey!![img]` ambiguity).
5. Collapse `\n{3,}` → `\n\n`.
6. Append accumulated footnotes (if any, when not consumed by `block_footnote_list`).
7. `trim()`.

---

## 3. Pipeline Integration with `lib.rs`

### Where to call

Defuddle calls `createMarkdownContent(result.content, url)` **after** `_standardize`, **after** clutter removal, on the *cleaned, standardized HTML string* — not raw input. See `markdown.ts:803-813` and Defuddle's `defuddle.ts` flow.

In Trek's `lib.rs::parse_internal` (currently lines 146–195), the equivalent point is **after** `standardize::standardize_content` produces `final_content`, but **before** building `TrekResponse`:

```rust
// existing
let final_content =
    standardize::standardize_content(&cleaned_content, &metadata.title, self.options.debug);

// NEW — Track B
let content_markdown = if self.options.output.markdown
    || self.options.output.separate_markdown
{
    let md_opts = markdown::MarkdownOptions {
        base_url: self.options.url.clone(),
        strip_leading_h1: true,
        ..Default::default()
    };
    Some(markdown::html_to_markdown(&final_content, &md_opts))
} else {
    None
};

// existing
let mut final_metadata = metadata.clone();
final_metadata.word_count = utils::count_words(&final_content);
// ...

Ok(TrekResponse {
    content: if self.options.output.markdown {
        content_markdown.clone().unwrap_or_default()
    } else {
        final_content
    },
    content_markdown: if self.options.output.separate_markdown {
        content_markdown
    } else {
        None
    },
    // ...
})
```

The site-specific extractor branch (`lib.rs:78-108`) needs the same insertion: after `extracted.content_html` is obtained.

### Word count

Defuddle counts words on the HTML. Keep our existing behaviour — `word_count` stays computed against `final_content` (HTML), not markdown. Markdown-mode users still get a meaningful count.

### Smart-retry interaction (`lib.rs:113-141`)

The retry path re-runs `parse_internal`; markdown will be regenerated on the retry result automatically. No extra wiring needed.

---

## 4. Required Type Changes

### `TrekResponse` — already has the field

`src/types.rs:127` already declares `pub content_markdown: Option<String>`. Currently always `None`. No struct change required; just start populating it.

### `TrekOptions::OutputOptions` — already has the fields

`src/types.rs:28-34` already declares:
```rust
pub markdown: bool,           // replace HTML content with markdown
pub separate_markdown: bool,  // include both HTML and markdown
```

These match Defuddle's `markdown` and `separateMarkdown`. No field additions needed.

### `MarkdownOptions` — new public struct in `markdown.rs`

Not exposed via WASM initially. Internally constructed from `TrekOptions`. If/when users want to tune marker style, expose via a follow-up.

### WASM bindings (`src/wasm.rs`)

If `output.markdown` is `true`, the existing JS `content` field already returns markdown (because we overwrite it in the integration step above). If `output.separate_markdown` is `true`, the new `contentMarkdown` field is non-null. No new WASM binding code required — `serde-wasm-bindgen` already serializes `Option<String>` as `null | string`.

---

## 5. Edge Cases to Mirror

Evidence cited as `expected/<file>.md` for each.

1. **Obsidian callouts** (`callouts--obsidian-publish-callouts.md`):
   - `> [!info] Title` then quoted body lines.
   - Fold indicators: `> [!faq]- Title` (collapsed), `+` (open), absent (not foldable).
   - Title taken from `.callout-title-inner`; if missing, capitalize the type.
   - Defuddle assumes upstream `callouts.ts` has standardized GitHub alerts, Bootstrap alerts, and Hugo admonitions into `div.callout[data-callout]`. Trek's `standardize.rs` already covers some — verify before this work lands.

2. **Footnotes** (`footnotes--aside-ol-start.md`):
   - In-text refs: `<sup id="fnref:1">` → `[^1]`. The id may have a suffix (`fnref:1-2`); only the part before `-` is used.
   - List: `<ol>` whose parent `id="footnotes"` → each `<li id="fn:N">` becomes `[^n]: <content>`; trailing `↩︎` and backref `<sup>` matching the id are stripped.
   - Wikipedia variant: id like `cite_note-name` → use the part after `cite_note-`.

3. **Code fence language detection** (`codeblocks--rehype-pretty-code.md`, `codeblocks--mintlify.md`, etc.):
   - Order: `<code data-lang>` → `<code data-language>` → `class="language-xxx"` regex → `<pre data-language>` → empty.
   - The expected output uses ` ```fish ` even though source has `<pre><code class="language-fish">`.
   - Inner text (after Shiki/highlight wrappers like `<span data-line>`) should be reduced to plain text — i.e., text-content extraction inside `<pre><code>`, not recursive markdown conversion.
   - Backticks inside content must be escaped (`\``); Defuddle does it before emission.

4. **Image figures** (`elements--lazy-image.md`, `elements--figure-content-wrapper.md`):
   - Standard figure: `![alt](src)\n\n<caption>\n\n`.
   - But if `<figure>` has `<p>` outside `<figcaption>`, treat as a content wrapper and recurse — DO NOT collapse. Several Medium/Substack layouts wrap whole sections in `<figure>`.
   - `srcset` with width descriptors: pick highest `Nw`. CDN URLs may contain commas, so tokenize on whitespace, not commas (port lines 41–77 verbatim).
   - Skip lazy-load placeholders: ignore data URIs and 1×1 pixels (already handled in `extract_first_image_from_content`; share logic).

5. **MathML / KaTeX** (`issues--141-arxiv-equation-tables.md`):
   - Block math: `\n$$\n<latex>\n$$\n`.
   - Inline math: `$<latex>$` with smart spacing — only insert leading/trailing space if the previous/next char isn't whitespace or `$`.
   - LaTeX source priority: `data-latex` → `alttext` → `annotation[encoding="application/x-tex"]` → `textContent` fallback.
   - Math inside table cells must stay inline (`closest('table')` check).
   - arXiv equation tables (`table.ltx_equation`) are unwrapped to display math.

6. **Tables** (`elements--complex-tables.md`):
   - Simple → pipe table with header separator. Cell newlines collapsed to spaces; `|` escaped as `\|`.
   - Complex (any `colspan`/`rowspan`) → emit cleaned raw HTML, attribute allowlist: `src,href,style,align,width,height,rowspan,colspan,bgcolor,scope,valign,headers`.
   - Layout tables (no nested tables, all rows have ≤1 cell, single column) → unwrap and render cell content as if it were the surrounding flow.
   - Empty tables → drop.

7. **Nested lists** (multiple expected files):
   - Tab indentation per nesting level (`\t.repeat(level - 1)`).
   - Continuation lines inside a single `<li>` get one extra tab.
   - Ordered lists honour `<ol start="N">` and item index.
   - Task lists: `<li class="task-list-item">` with `<input type="checkbox" [checked]>` → `[x] ` or `[ ] ` after the bullet.

8. **Embeds**: YouTube `<iframe>` → `![](https://www.youtube.com/watch?v=ID)`. Twitter/X `<iframe>` → `![](https://x.com/USER/status/ID)` or `![](https://x.com/i/status/ID)`. Pattern in lines 346–376.

9. **`<wbr>` removal**: pre-pass strip before parsing — otherwise html5ever inserts a token that the renderer turns into a space.

10. **`! ![img]` ambiguity fix**: post-process insert space.

11. **Empty link removal**: post-process drop `[](url)` (but keep `![](url)`).

---

## 6. File List

All paths absolute under `/Users/christopherw/Workspaces/officialunofficial/trek/`.

| Path | Purpose |
|---|---|
| `src/markdown.rs` | New module. `MarkdownOptions`, `html_to_markdown`, `Renderer` state machine, all block/inline handlers, post-processor. ~700–900 lines. |
| `src/markdown/handlers.rs` | (Optional split) Per-tag handlers if `markdown.rs` exceeds ~600 lines. Same module via `pub(crate) mod handlers;`. |
| `src/markdown/srcset.rs` | (Optional split) `pick_best_src(srcset, src)` ported from `getBestImageSrc`. Reusable by `extract_first_image_from_content` in `lib.rs`. |
| `src/markdown/footnotes.rs` | (Optional split) `Footnotes` struct: collects refs during walk, emits trailing list. |
| `src/lib.rs` | Add `pub mod markdown;`. Insert markdown call after `standardize_content` in `parse_internal` (and in the site-extractor branch). |
| `src/types.rs` | No structural change; `content_markdown` and the two flags already exist. Optionally add doc comments noting Defuddle-equivalent flags. |
| `Cargo.toml` | Add `html5ever = "0.27"`, `markup5ever_rcdom = "0.3"`. |
| `tests/markdown_integration.rs` | New integration tests. Mirror Defuddle's `tests/markdown.test.ts` cases (`!![img]`, `wbr`, base href). Add at least one fixture per edge case from §5. |
| `tests/fixtures/markdown/*.html` and `tests/fixtures/markdown/*.expected.md` | Golden test fixtures. Seed by porting ~5 Defuddle expected files (callout, footnote, code-block, math, table). |
| `docs/refactor/track-b-markdown.md` | This file. |
| `docs/api-reference.md` | Update to document `markdown` / `separateMarkdown` options now actually doing something, and `contentMarkdown` response field. |
| `Makefile` | No change required; existing `make test` covers new tests. |

---

## Open Questions / Follow-ups

- **MathML→LaTeX fallback**: Defuddle uses npm `mathml-to-latex` when no `data-latex`/`alttext` is present. There is no maintained Rust port. Options: (a) ship without fallback (most arXiv/Wikipedia content has `alttext`); (b) embed a hand-rolled subset converter for the ~30 most common MathML elements; (c) call out to a JS shim only in WASM via `wasm-bindgen`. Recommendation: ship (a), file follow-up for (b).
- **Streaming alternative**: if memory becomes an issue (it won't for typical articles, but might for extracted long-form), revisit a `lol_html`-based emitter for the simpler subset.
- **Markdown-of-extractor-output**: site-specific extractors return `content_html`. They should also benefit from markdown conversion. The integration in §3 covers this — verify when implementing.
- **Round-trip test**: pipe Trek output through `pulldown-cmark` and snapshot the rendered HTML to catch malformed markdown early. Cheap insurance.
