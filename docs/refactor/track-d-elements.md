# Track D — Element Handlers

Status: spec / not yet implemented
Scope: bring `src/elements/` to feature-parity with Defuddle, add **callouts** and **math**, and define the bundle-tier story for math.

Defuddle source of truth: `/tmp/defuddle-clone/src/elements/` (TS, DOM-mutating).
Trek today: `/Users/christopherw/Workspaces/officialunofficial/trek/src/elements/` (lol_html-based, partial).

---

## 1. Per-element gap analysis

### 1.1 callouts (NEW in Trek)

Defuddle (`callouts.ts`, ~138 LOC) standardizes 5 callout flavors **before** the selector-removal step (otherwise `.alert`/`.admonition` get stripped):

| Source | Detection | Output |
|---|---|---|
| Obsidian Publish | `.callout.is-collapsed`, `.callout.is-collapsible` | unwrap `is-collapsed`/`is-collapsible`, preserve `data-callout-fold`, strip inline `display:none` |
| GitHub markdown alerts | `.markdown-alert` + `.markdown-alert-{type}` class | rebuild as `<div data-callout="{type}" class="callout">` w/ `.callout-title` + `.callout-content` |
| `aside.callout-*` | class prefix `callout-` | same rebuild |
| Hugo / Docsy admonitions | `.admonition` + type class (`info`, `warning`, `note`, `tip`, `danger`, `caution`, `important`, `abstract`, `success`, `question`, `failure`, `bug`, `example`, `quote`) | same rebuild; pulls title from `.admonition-title`, content from `.admonition-content`/`.details-content` |
| Bootstrap | `.alert.alert-*` (excludes `alert-dismissible`) | same rebuild; pulls title from `.alert-heading`/`.alert-title` |

Output shape (target HTML, identical for all sources):
```html
<div data-callout="warning" class="callout">
  <div class="callout-title"><div class="callout-title-inner">Warning</div></div>
  <div class="callout-content"><!-- transferred --></div>
</div>
```

**Trek today: nothing.** No file. Several fixtures depend on this:
- `callouts--obsidian-publish-callouts.html`
- `elements--bootstrap-alerts.html`
- `elements--hugo-admonitions.html`

### 1.2 code (gap: large)

Defuddle `code.ts` (~515 LOC) is a single rewrite-rule that:
- Matches a wide selector union: `pre`, `div[class*="prismjs"]`, `.syntaxhighlighter`, `.highlight`, `.highlight-source`, `.wp-block-syntaxhighlighter-code`, `.wp-block-code`, `div[class*="language-"]`, `.code-block[data-lang]` (Writerside), `code.hl.block` (Verso/Lean).
- Removes `<button>` / `[class*="codeblock-button"]` chrome.
- Removes header/toolbar/titlebar siblings (≤5 words, no semantic children).
- Detects language via:
  - `data-lang`/`data-language`/`language` attrs,
  - 8 class-name regex patterns (`language-X`, `lang-X`, `X-code`, `code-X`, `syntax-X`, `code-snippet__X`, `highlight-X`, `X-snippet`),
  - bare-name fallback against an ~80-entry allowlist (`CODE_LANGUAGES`).
- Detects CodeMirror containers (e.g. ChatGPT) and pulls language from neighboring header text.
- Has bespoke extractors for WordPress SyntaxHighlighter (table + non-table), Hugo/Chroma `span.lnt`, Pygments `span.lineno`, react-syntax-highlighter, Rouge `.rouge-gutter`, two-child gutter pattern, and generic `[data-line]`/`.line` line containers.
- Skips Verso `.hover-info`/`.hover-container` tooltips.
- Cleans up: tabs→4 spaces, NBSP→space, dedent (common-leading-whitespace removal), normalize `\n{3,}`→`\n\n`, trim. Verso path keeps trailing newlines.
- Cleans up code-block sibling chrome up to 3 ancestors (skips inside `[data-callout]`).
- Emits `<pre><code class="language-X" data-lang="X">{content}</code></pre>`.

**Trek today** (`code.rs`, 60 LOC): only preserves `class` attr on `pre`/`code` and strips other attrs. No language detection, no syntax-highlighter recognition, no line-number stripping, no chrome removal, no dedent. Diff = effectively the entire 515-line module.

Fixture coverage (~17 fixtures):
- `codeblocks--chatgpt-codemirror.html`, `codeblocks--chroma-{linenums,inline-linenums,line-spans}.html`, `codeblocks--code-pre-nesting.html`, `codeblocks--flex-row-gutter.html`, `codeblocks--hexo-br.html`, `codeblocks--hljs-header.html`, `codeblocks--mintlify.html`, `codeblocks--pygments-lineno.html`, `codeblocks--react-syntax-highlighter-linenums.html`, `codeblocks--rehype-pretty-{code,copy}.html`, `codeblocks--rouge-linenums.html`, `codeblocks--rockthejvm.com-...`, `codeblocks--stripe.html`, `code-blocks--chroma-linenums.html`, `code-blocks--hexo-br.html`.

### 1.3 footnotes (gap: very large)

Defuddle `footnotes.ts` (~1235 LOC) is a `FootnoteHandler` class with:
- 3 inline-sidenote collectors (inline, sidenotes-column, aside `<ol start>`, hidden-aside `data-definition`).
- `collectFootnotes` traversing `FOOTNOTE_LIST_SELECTORS` with branches for: Wikidot `div.footnotes-footer`, pulldown-cmark `div.footnote-definition`, Hugo/org-mode `div.footnote-definitions`, Easy Footnotes WP, Substack inline footnote divs, generic `<li>`/`[role=listitem]`.
- 6 fallbacks (`tryGenericIdDetection`, `tryWordExport`, `tryGoogleDocs`, `tryLabeledSection`, `tryLooseFootnotes`, `tryClassFootnote`) with 75% cross-validation thresholds.
- 13-entry `INLINE_REF_EXTRACTORS` table covering Nature, Science.org, Substack, MediaWiki, Markdown-It, LessWrong, etc.
- Special handling for arxiv multi-citations (`cite.ltx_cite`).
- Outputs a normalized `<div id="footnotes"><ol>...</ol></div>` and rewrites all inline references into `<sup id="fnref:N"><a href="#fn:N">N</a></sup>` with `↩` backrefs.

**Trek today** (`footnotes.rs`, 135 LOC): only adds `data-footnote=true` markers to `<a>` whose `href` starts with `#fn`/`#cite`/`#reference`/`#footnote`, plus marks `ol.footnotes` with `data-footnote-list=true`. **No collection, no normalization, no rewriting.**

Fixture coverage: ~30 fixtures named `footnotes--*` (plus `issues--120-dhammatalks-footnotes.html`, `issues--218-footnote-wrapper-text-lost.html`).

### 1.4 headings (gap: medium)

Defuddle `headings.ts` (~113 LOC):
- `removePermalinkAnchors`: detect anchors via class (`permalink`, `anchor-link`, `heading-anchor`), title (contains "permalink"), bare `#`/`¶`/`§`/`🔗`/BOM text, or `href` starting with `#`.
- `headingRules` rewrite for `h1`–`h6`: rebuild a clean heading; allowed-attribute filter; remove `<button>`, permalink anchors, `.anchor`, `.permalink-widget`; recover lost text from removed nav elements.

**Trek today** (`headings.rs`, 88 LOC): handles `div[role=heading]` detection (but cannot rename in lol_html so notes the limitation), strips non-`id`/`class` attrs from `h1`–`h6`, has a `process_h1_element` stub. Missing: permalink-anchor removal, button removal, nav-element detection, allowed-attribute filtering.

Fixtures: `headings--fragment-url-not-permalink.html`, `headings--permalink-title-match.html`, `headings--testid-article-header.html`, `issues--159-lean-heading-permalink-emoji.html`.

### 1.5 images (gap: large)

Defuddle `images.ts` (~1004 LOC) implements 5 selector rules:
1. `picture`: pick best `<source>` (default → highest-resolution by `Nw` × `dpr=N`), copy srcset onto inner `<img>`, drop `<source>` siblings.
2. `uni-image-full-width` custom element → `<figure>` (parses `data-loading` JSON for desktop URL).
3. Lazy-image rule: `img[data-src]`, `img[data-srcset]`, `img[loading="lazy"]`, `img.lazy`, `img.lazyload`, `img[src^="data:image/svg+xml"]` — promote `data-src`/`data-srcset` to `src`/`srcset`, scan all attrs for image-URL patterns, drop placeholders, strip `lazy*` classes/attrs.
4. `span:has(img)` wrappers → caption-detect → emit `<figure><figcaption>`.
5. `figure, p:has([class*="caption"])` standardization with caption discovery (figcaption, `[class*="caption|description|credit|alt|title|text|aria-label|title]`, sibling search, sup/em/cite trailing).
- Supporting helpers: base64-placeholder detection (≤133 chars), srcset URL extraction tolerant of CDN URLs containing commas (Substack), `selectBestSource` with width/DPR scoring.

**Trek today** (`images.rs`, 128 LOC): minimum-dimension filter (50×50), tracking-pixel keyword filter (`pixel`/`tracking`/`analytics`/`1x1`), strips attributes outside `["src","alt","width","height","srcset"]`. Missing: lazy-image promotion, picture/source resolution, caption/figure synthesis, custom-element handling, base64 placeholder detection, comma-tolerant srcset parsing.

Fixtures: `elements--lazy-image.html`, `elements--svg-placeholder-lazy-image.html`, `elements--image-dedup.html`, `elements--lightbox-image-dedup.html`, `elements--srcset-normalization.html`, `elements--base64-placeholder-removal.html`, `elements--figure-content-wrapper.html`, `issues--221-nextjs-noscript-images.html`, `issues--227-noscript-lazy-images.html`.

---

## 2. Math: tiers, sub-modules, and bundling

### 2.1 The Defuddle math module

Three sibling files share a base:

- **`math.base.ts`** (~419 LOC, no deps): pure analyzers + the raw-LaTeX wrapper.
  - `getMathMLFromElement(el)` — checks 4 sources (direct `<math>`, `data-mathml`, MathJax assistive `<mjx-assistive-mml>` / `.MJX_Assistive_MathML`, `.katex-mathml math`).
  - `getBasicLatexFromElement(el)` — `data-latex`, `data-math`, WP `img.latex` (alt or URL `latex.php?latex=`), `<annotation encoding="application/x-tex">`, `.katex` annotation, `script[type="math/tex"]`, sibling math-script lookup, `<math>` textContent fallback, `alt`.
  - `isBlockDisplay(el)` — explicit `display=block`, class includes `display`/`block`, ancestor `.katex-display`/`.MathJax_Display`, preceding `<p>`, `.mwe-math-fallback-image-display`, KaTeX nesting check, MathJax v3.
  - `mathFastCheck` — cheap selector for early-exit.
  - `mathSelectors` — full union of recognized math markup (~30 selectors covering MathJax v2/v3, MediaWiki, KaTeX, generic `<math>`/`data-*`).
  - `extractLatexFromImageSrc(src)` — pulls LaTeX from `?latex=`/`?tex=`/`?eq=`/`?math=`/`?chl=` query params, full query, or URL-encoded path segments (CodeCogs, mimeTeX, Google Charts).
  - `wrapRawLatexDelimiters(element, doc)` — scans text nodes for `$…$`, `$$…$$`, `\(…\)`, `\[…\]` and wraps matches in `<math data-latex>`. Gated by `hasMathLibrary(doc)` (page must include MathJax/KaTeX script) so `$` doesn't get parsed as currency.

- **`math.core.ts`** (~65 LOC, no deps): assembles `mathRules` using **only** `getBasicLatexFromElement`. No MathML→LaTeX conversion. ~ Browser core bundle.

- **`math.full.ts`** (~101 LOC, depends on `mathml-to-latex` + `temml`):
  - `getLatexFromElement` = basic LaTeX, falling back to `MathMLToLaTeX.convert(mathData.mathml)`.
  - `createCleanMathEl` additionally renders LaTeX → MathML via `temml` so output always has structured MathML even when only LaTeX was found.
  - Exports `mathRules` keyed off `mathSelectors`/`mathFastCheck` like `math.core` but using the richer extractor.

- **`math.ts`** (~6 LOC) — Node/tsc entrypoint that re-exports `math.full`. Webpack `alias` swaps to `math.core` for the browser core bundle, leaves it as `math.full` for the full bundle.

So Defuddle ships **three flavors**:
| Bundle | math impl | output guarantees |
|---|---|---|
| `index.full.ts` (Node + browser-full) | `math.full` | always emits `<math>` with both MathML body and `data-latex`, converting either direction |
| browser core | `math.core` | emits `<math>` with `data-latex` from native sources only; MathML preserved when present |
| (none) | `math.base` alone | building block — never registered as a rule on its own |

### 2.2 Recommended Trek tiering

Trek does not need to copy the webpack-alias trick. Cargo features map cleanly:

| Cargo feature | Default? | Pulls in | Behavior |
|---|---|---|---|
| `math-base` | yes | nothing extra | analyzers + raw-LaTeX text scanner; rules emit `<math data-latex>` from sources that already expose LaTeX (KaTeX annotation, MathJax script, `data-latex`, WP `img.latex`). MathML pass-through but no MathML↔LaTeX conversion. |
| `math-mathml-to-latex` | no | a Rust MathML→LaTeX converter (candidate: `mathml-rs` if maintained, else port a minimal subset) | adds the "MathML present, no LaTeX" branch — converts to LaTeX before emit. |
| `math-latex-to-mathml` | no | a temml-equivalent (no Rust port today; either WASM-import temml at runtime via `js-sys` for WASM target, or skip on native) | adds the "LaTeX only" → MathML render branch. |
| `math-full` | no | `math-mathml-to-latex` + `math-latex-to-mathml` | parity with Defuddle's `index.full`. |

Recommendation:
- Make `math-base` a non-default but stable feature flag for v0.3 (most readers don't need it; opting in is cheap — pure Rust, no JS deps).
- Keep `math-mathml-to-latex` and `math-latex-to-mathml` as separate features so consumers pay only for what they need. WASM size delta should be measured per feature in CI.
- `math-full` is a convenience alias enabling all three.
- `math` umbrella feature defaults to `math-base` only. Document trade-off in README.

WASM-size estimate (rough): `math-base` adds ~6–10 KB gz (selectors + analyzers); `math-mathml-to-latex` +30–60 KB depending on chosen crate; `math-latex-to-mathml` is the heaviest (port of temml is non-trivial — likely +80–150 KB or punt to JS interop on `wasm32-unknown-unknown`).

---

## 3. Recommended Rust module layout

Target tree:
```
src/elements/
├── mod.rs                      # public re-exports + ElementProcessor trait
├── callouts.rs                 # NEW — Track D core deliverable
├── code.rs                     # rewrite (currently ~60 LOC stub → ~600 LOC)
├── footnotes.rs                # rewrite (currently ~135 LOC stub → ~1100 LOC)
├── headings.rs                 # extend (currently ~88 LOC stub → ~150 LOC)
├── images.rs                   # rewrite (currently ~128 LOC stub → ~700 LOC)
└── math/
    ├── mod.rs                  # feature-gated re-exports
    ├── base.rs                 # math.base.ts port (always built)
    ├── core.rs                 # math.core.ts port (math-base feature)
    ├── full.rs                 # math.full.ts port (math-full feature)
    └── selectors.rs            # mathSelectors / mathFastCheck constants
```

Note: trek already operates against streaming `lol_html::html_content::Element`, but every Defuddle handler in scope here mutates the *whole subtree* and needs `closest()`/sibling navigation that lol_html does not provide on rewrites. **Action:** these handlers must run on a buffered DOM. Recommend:
- Materialize content into a `kuchikiki` (or `markup5ever_rcdom`) tree once after the first lol_html streaming pass collects metadata.
- All `standardize_*` functions in this track operate on that tree, returning a serialized HTML string.
- This matches Defuddle's two-phase pipeline and makes the ports much closer to the TS source.

### 3.1 Proposed function signatures

```rust
// src/elements/callouts.rs
pub fn standardize_callouts(root: &NodeRef);

// src/elements/code.rs
pub fn standardize_code_blocks(root: &NodeRef);
pub(crate) fn detect_language(el: &NodeRef) -> Option<String>;
pub(crate) fn extract_code_text(el: &NodeRef) -> String;

// src/elements/footnotes.rs
pub fn standardize_footnotes(root: &NodeRef);
pub(crate) struct FootnoteHandler<'a> { /* doc, pending_removals */ }
impl<'a> FootnoteHandler<'a> {
    pub(crate) fn collect_footnotes(&mut self, root: &NodeRef) -> FootnoteCollection;
    pub(crate) fn collect_inline_sidenotes(&mut self, root: &NodeRef) -> FootnoteCollection;
    pub(crate) fn collect_sidenotes_column(&mut self, root: &NodeRef) -> FootnoteCollection;
    pub(crate) fn collect_aside_footnotes(&mut self, root: &NodeRef) -> FootnoteCollection;
    pub(crate) fn collect_hidden_aside_footnotes(&mut self, root: &NodeRef) -> FootnoteCollection;
    /* private: try_generic_id_detection, try_word_export, try_google_docs,
                try_labeled_section, try_loose_footnotes, try_class_footnote */
}

// src/elements/headings.rs
pub fn remove_permalink_anchors(root: &NodeRef);
pub fn standardize_headings(root: &NodeRef);
pub fn is_permalink_anchor(node: &NodeRef) -> bool;

// src/elements/images.rs
pub fn standardize_images(root: &NodeRef);
pub(crate) fn process_picture(el: &NodeRef);
pub(crate) fn promote_lazy_image(el: &NodeRef);
pub(crate) fn span_image_to_figure(el: &NodeRef);
pub(crate) fn standardize_figure(el: &NodeRef);
pub(crate) fn extract_first_url_from_srcset(srcset: &str) -> Option<&str>;
pub(crate) fn is_base64_placeholder(src: &str) -> bool;
pub(crate) fn select_best_source<'a>(sources: &'a [NodeRef]) -> Option<&'a NodeRef>;

// src/elements/math/base.rs
pub struct MathData { pub mathml: String, pub latex: Option<String>, pub is_block: bool }
pub fn get_mathml_from_element(el: &NodeRef) -> Option<MathData>;
pub fn get_basic_latex_from_element(el: &NodeRef) -> Option<String>;
pub fn is_block_display(el: &NodeRef) -> bool;
pub fn extract_latex_from_image_src(src: &str) -> Option<String>;
pub fn wrap_raw_latex_delimiters(root: &NodeRef);
pub const MATH_FAST_CHECK: &str;
pub const MATH_SELECTORS: &str;

// src/elements/math/core.rs   (cfg(feature = "math-base"))
pub fn standardize_math(root: &NodeRef);
pub fn create_clean_math_el(data: Option<&MathData>, latex: Option<&str>, is_block: bool) -> NodeRef;

// src/elements/math/full.rs   (cfg(feature = "math-full"))
pub fn standardize_math(root: &NodeRef);   // shadows core when full is enabled
pub fn get_latex_from_element(el: &NodeRef) -> Option<String>;
```

Conditional compilation in `mod.rs`:
```rust
pub mod base;
#[cfg(any(feature = "math-base", feature = "math-full"))]
mod core;
#[cfg(feature = "math-full")]
mod full;
#[cfg(feature = "math-full")]
pub use full::standardize_math;
#[cfg(all(feature = "math-base", not(feature = "math-full")))]
pub use core::standardize_math;
```

---

## 4. Cargo features to add

Append to `[features]` in `Cargo.toml` (currently empty):

```toml
[features]
default = []
math-base = []
math-mathml-to-latex = ["dep:mathml-converter-crate"]   # crate TBD
math-latex-to-mathml = []                                # WASM-only via temml interop initially
math-full = ["math-base", "math-mathml-to-latex", "math-latex-to-mathml"]
```

(Crate selection is a follow-up — `mathml-rs` and `mathyank` are candidates but neither currently has a stable Rust→LaTeX conversion. If none ships, port a minimal subset locally and gate it behind `math-mathml-to-latex` as a vendored module.)

WASM size guardrails (CI): publish gz size for `default`, `math-base`, `math-full`. Block PRs that grow `default` past +5 KB.

---

## 5. Concrete file list

Files to **create**:
- `src/elements/callouts.rs` — Track D core deliverable; ports `callouts.ts`. ~150 LOC.
- `src/elements/math/mod.rs` — feature-gated module surface. ~25 LOC.
- `src/elements/math/base.rs` — analyzers + selectors + raw-LaTeX scanner. ~450 LOC.
- `src/elements/math/core.rs` — `math.core.ts` port. ~80 LOC.
- `src/elements/math/full.rs` — `math.full.ts` port (behind feature). ~120 LOC.
- `tests/fixtures/elements_*.rs` — fixture-driven tests.

Files to **rewrite** (existing stubs are placeholders, not extensions):
- `src/elements/code.rs` — full code-block normalizer (~600 LOC).
- `src/elements/footnotes.rs` — full footnote handler (~1100 LOC).
- `src/elements/images.rs` — full image/figure pipeline (~700 LOC).

Files to **extend** (keep current shape, add missing behavior):
- `src/elements/headings.rs` — add permalink-anchor removal + nav-element collection (+~80 LOC).
- `src/elements/mod.rs` — add `pub mod callouts;`, `pub mod math;`. Replace `ElementProcessor` trait with `pub trait Standardize { fn run(&self, root: &NodeRef); }` once we move off lol_html for this track.

Files to **touch elsewhere**:
- `Cargo.toml` — add `[features]` block (Section 4) and add chosen DOM crate (`kuchikiki = "0.8"` recommended).
- `src/lib.rs` — wire the standardize_* calls into the post-streaming pass between `cleaned_content` and `standardize::standardize_content`.
- `Makefile` — add `wasm-build-math` and `wasm-build-math-full` targets and a `make size` budget check.

---

## 6. Open questions

1. **DOM library**: lol_html cannot do `closest()`, sibling walks, or replace-and-rewire that these handlers all need. Decision required: `kuchikiki` vs. `markup5ever_rcdom` directly vs. `scraper`. Recommend `kuchikiki` (selector engine + tree mutation; ~140 KB on WASM, comparable to current lol_html footprint).
2. **`math-latex-to-mathml`**: no pure-Rust temml exists. Three options — (a) skip on WASM, return LaTeX-as-text only; (b) ship temml-via-JS as part of the WASM bundle (uses `js-sys`/`web-sys`, needs host shim); (c) port a tiny subset (only the macros that defuddle ever sees). Lean toward (a) for v0.3 and revisit.
3. **Order of standardize_* calls** in the pipeline: callouts first (before selector removal), then math (so wrapped LaTeX is in the tree before code/headings touch text), then images, then code, then headings, then footnotes (last — needs final IDs). Document this in `lib.rs` comments.
