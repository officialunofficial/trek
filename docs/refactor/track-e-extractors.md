# Track E — Site-Specific Extractors

Status: spec / not yet implemented
Scope: port all 25 Defuddle extractors + 2 abstract bases to Trek and wire registry precedence.

Defuddle source: `/tmp/defuddle-clone/src/extractors/` and `extractor-registry.ts`.
Trek today: `src/extractor.rs` (trait + registry skeleton, only `GenericExtractor`), `src/extractors/mod.rs` (empty).

---

## 1. The 25 extractors at a glance

LOC = TS source LOC. Complexity buckets: S ≤ 100, M ≤ 250, L ≤ 500, XL > 500.

| # | Name | Family | URL / DOM trigger | LOC | Cx | One-line description |
|---|---|---|---|---:|:--:|---|
| 1 | `XArticleExtractor` | social | `x.com`/`twitter.com` + `articleContainer` DOM probe | 328 | L | Long-form X "Article" pages — extracts author + body + media. Registered **before** Twitter so it wins on long-form posts. |
| 2 | `TwitterExtractor` | social | `twitter.com`, `/x.com/.*/` + `mainTweet` | 332 | L | Classic tweet thread + replies; classifies cellInnerDiv into thread vs. reply. |
| 3 | `XOembedExtractor` | social | `x.com`/`twitter.com`, **async-only** (`canExtract`=false, `canExtractAsync`=true) | 531 | XL | Async fallback fetching FxTwitter / oEmbed — used when DOM is unavailable. |
| 4 | `RedditExtractor` | social | `reddit.com`, `old.reddit.com`, `new.reddit.com`, `*.reddit.com` + `shredditPost`/`isOldReddit` | 231 | M | Post body + comments; async fetches old.reddit.com server-side for comments. |
| 5 | `HackerNewsExtractor` | news | `news.ycombinator.com` + `mainPost`/`isListingPage` | 296 | L | Story page, comment page, and listing page; rebuilds nested comment tree. |
| 6 | `ChatGPTExtractor` | ai-chat | `^https?://chatgpt\.com/(c\|share)/.*$` + `turns` | 158 | M | Conversation transcript via `[data-message-author-role]`; rewrites citation containers. |
| 7 | `ClaudeExtractor` | ai-chat | `claude.ai`, `^https?://claude\.ai/(chat\|share)/.*$` + `articles` | 99 | S | Two-author chat; reads `[data-testid=user-message]` + `.font-claude-message`. |
| 8 | `GrokExtractor` | ai-chat | `^https?://grok\.com/(chat\|share)(/.*)?$` + `messageBubbles` | 164 | M | DOM is utility-class soup (`items-end`/`items-start`); brittle but documented. |
| 9 | `GeminiExtractor` | ai-chat | `^https?://gemini\.google\.com/app/.*$` + `conversationContainers` | 130 | M | Conversation container walk; carves out `table-content` collisions with Defuddle. |
| 10 | `GitHubExtractor` | dev | `github.com` + meta-tag/CSS-class indicators + issue/PR test-IDs | 293 | L | Issues, PRs, code review threads; converts highlight-source-{lang} to standard pre/code. |
| 11 | `LeetCodeExtractor` | dev | `leetcode.com` + `[data-track-load="description_content"]` | 23 | S | Tiny `contentSelector`-only extractor. |
| 12 | `LwnExtractor` | news | `lwn.net` + `.PageHeadline` + `.ArticleText` | 135 | M | Article body + comment hierarchy. |
| 13 | `WikipediaExtractor` | knowledge | `wikipedia.org` + `#mw-content-text` | 24 | S | `contentSelector`-only; just normalizes title (strips `– Wikipedia`). |
| 14 | `MediumExtractor` | knowledge | `medium.com`, `*.medium.com` + meta og:site_name="Medium" | 140 | M | Article body; unwraps `[role=button]` image wrappers; fetches description before clean. |
| 15 | `SubstackExtractor` | knowledge | `substack.com/@u/note/...`, `substack.com/home/post/p-N`, `substack.com` + post or note selector | 211 | M | Posts, notes, custom-domain Substacks; reads `_preloads` for SSR fallback. |
| 16 | `NytimesExtractor` | news | `nytimes.com` + JSON content blob | 294 | L | Renders NYT JSON content blocks; image-rendition picker (superJumbo > jumbo > articleLarge). |
| 17 | `LinkedInExtractor` | social | `linkedin.com` + `postArticle` | 257 | L | Post body only (not feed); strips reposts, sr-only dupes, internal LinkedIn attrs. |
| 18 | `ThreadsExtractor` | social | `threads.net`, `*.threads.com` + pagelets or region container | 517 | XL | Server-rendered + pagelet variants; classifies thread vs. reply by author. |
| 19 | `BlueskyExtractor` | social | `bsky.app` + `postItems` | 334 | L | Connector-line heuristic to classify thread depth. |
| 20 | `MastodonExtractor` | social | path matches `/@username/123` + DOM markers | 280 | L | Self-reply chains form thread; unwraps URL-truncating spans. |
| 21 | `DiscourseExtractor` | social | path matches `/t/<slug>/<id>` + Discourse markers + `.topic-post` | 172 | M | OP + replies; strips selection barriers + heading anchors. |
| 22 | `YoutubeExtractor` | other | `youtube.com`, `youtu.be` + watch URLs | 1266 | XL | Video transcript via unofficial InnerTube Android-client API; complex caption-track selection + sentence segmentation (Latin + CJK). Has `prefersAsync` = true. |
| 23 | `C2WikiExtractor` | knowledge | `wiki.c2.com` (canExtract=false, async-only) | 164 | M | Sync extraction is a no-op; async pulls the markdown source from `?action=source`. |
| 24 | `BbcodeDataExtractor` | other | matches **everything** (`/.*/`); requires `#application_config[data-partnereventstore]` | 58 | S | Steam group announcement BBCode renderer; lowest-precedence catch-all. |
| 25 | `XOembedExtractor` already counted | — | — | — | — | — |

(Defuddle's `extractor-registry` registers 24 entries; XOembed slots between Twitter and Reddit. The `_base.ts` and `_conversation.ts` files are abstract bases and don't count toward the 25.)

### Family roll-up

- **social** (8): X-Article, Twitter, X-Oembed, Reddit, LinkedIn, Threads, Bluesky, Mastodon, Discourse
- **news** (3): HackerNews, LWN, NYTimes
- **ai-chat** (4): ChatGPT, Claude, Grok, Gemini
- **dev** (2): GitHub, LeetCode
- **knowledge** (4): Wikipedia, Medium, Substack, C2-Wiki
- **other** (3): YouTube, BbcodeData, (Generic fallback in Trek)

---

## 2. Trait additions / changes

Trek's current `Extractor` trait:
```rust
pub trait Extractor: Send + Sync {
    fn can_extract(&self, url: &str, schema_org_data: &[Value]) -> bool;
    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent>;
    fn name(&self) -> &'static str;
}
```

This is **insufficient** for parity. Defuddle's `BaseExtractor` exposes:
- `canExtract()` — sync DOM probe
- `canExtractAsync()` — async DOM-or-network probe (default false)
- `prefersAsync()` — even when sync works, prefer async (default false; YouTube=true)
- `extract()` — sync extraction
- `extractAsync()` — async extraction (default delegates to `extract`)
- A shared `fetch` reference (overridable from `ExtractorOptions`)

And there's a second-tier `ConversationExtractor` (chat extractors) that:
- declares `extractMessages()`, `getMetadata()`, `getFootnotes()`,
- generates uniform message HTML,
- runs the host pipeline (`Defuddle`) recursively on the synthesized HTML,
- emits a `messageCount` `extractedContent` value.

### 2.1 Recommended Rust trait redesign

```rust
// src/extractor.rs

#[derive(Debug, Default, Clone)]
pub struct ExtractorOptions {
    pub include_replies: ReplyMode,        // None | Always | ExtractorsOnly
    pub language: Option<String>,
    // fetch override is handled at the registry/host level — see below
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ReplyMode { #[default] None, Always, ExtractorsOnly }

#[derive(Debug, Default)]
pub struct ExtractorResult {
    pub content: String,
    pub content_html: String,
    pub content_selector: Option<String>,
    pub extracted_content: HashMap<String, String>,
    pub variables: ExtractorVariables,
}

#[derive(Debug, Default)]
pub struct ExtractorVariables {
    pub title: Option<String>,
    pub author: Option<String>,
    pub site: Option<String>,
    pub description: Option<String>,
    pub published: Option<String>,
    pub word_count: Option<String>,
}

pub trait Extractor: Send + Sync {
    fn name(&self) -> &'static str;

    /// URL/path match — runs before DOM is parsed. Equivalent to the patterns
    /// list in defuddle's registry. Default = match everything (let `can_extract` decide).
    fn url_matches(&self, url: &Url) -> bool { true }

    /// Cheap DOM probe. Trek currently only has the streamed metadata; this
    /// trait method gets the *parsed* DOM (kuchikiki tree) once Track D's DOM
    /// migration lands. Equivalent of defuddle's `canExtract()`.
    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool;

    fn extract(&self, ctx: &ExtractCtx<'_>) -> Result<ExtractorResult>;

    fn can_extract_async(&self, _ctx: &ExtractCtx<'_>) -> bool { false }
    fn prefers_async(&self) -> bool { false }
    fn extract_async<'a>(&'a self, ctx: &'a ExtractCtx<'a>) -> BoxFuture<'a, Result<ExtractorResult>> {
        Box::pin(async move { self.extract(ctx) })
    }
}

/// Shared context passed to every extractor — replaces (url, schema_org_data)
/// and adds parsed DOM access.
pub struct ExtractCtx<'a> {
    pub url: &'a Url,
    pub schema_org_data: &'a [Value],
    pub document: &'a NodeRef,                // kuchikiki tree
    pub options: &'a ExtractorOptions,
    pub fetch: Option<&'a dyn Fetcher>,       // injected by host (browser / native)
}
```

### 2.2 Conversation base

```rust
// src/extractors/_conversation.rs

pub struct ConversationMessage {
    pub author: String,
    pub content: String,
    pub timestamp: Option<String>,
    pub metadata: HashMap<String, String>,
}

pub struct ConversationMetadata {
    pub title: Option<String>,
    pub site: String,
    pub description: Option<String>,
}

pub trait ConversationExtractor: Extractor {
    fn extract_messages(&self, ctx: &ExtractCtx<'_>) -> Vec<ConversationMessage>;
    fn get_metadata(&self, ctx: &ExtractCtx<'_>) -> ConversationMetadata;
    fn get_footnotes(&self, _ctx: &ExtractCtx<'_>) -> Vec<Footnote> { Vec::new() }
}

/// Shared `extract` impl — generates uniform message HTML and re-runs the
/// host pipeline. Provided as a default associated function or a free fn:
pub fn extract_conversation<E: ConversationExtractor>(
    e: &E, ctx: &ExtractCtx<'_>
) -> Result<ExtractorResult> { /* port of _conversation.ts:extract */ }
```

The "re-run pipeline on synthesized HTML" trick requires Trek to expose its top-level extraction as a callable function (`crate::Trek::parse_html(html, opts)`). It already does internally — just needs a public-`pub(crate)` re-entrancy seam.

### 2.3 What changes in `extractor.rs`

- Replace `Extractor::can_extract(&self, url: &str, schema_org_data: &[Value])` with the `ExtractCtx`-based version above.
- Replace `Extractor::extract_from_html(&self, html: &str)` similarly.
- `ExtractorRegistry::find_extractor_from_data` becomes `find_sync_extractor(ctx) / find_async_extractor(ctx) / find_preferred_async_extractor(ctx)` mirroring Defuddle.
- Keep `GenericExtractor` but re-target: its `can_extract` should keep returning `false` and the host's main pipeline keeps acting as the genuine fallback.

---

## 3. Family groupings for parallel implementation

Designed so 4 agents can work concurrently with **zero file overlap** and minimal trait churn after Phase 0.

**Phase 0 (single agent, blocking):**
- Land the new `Extractor` trait, `ExtractCtx`, `ExtractorOptions`, `ExtractorResult` in `src/extractor.rs`.
- Land `_base.rs` and `_conversation.rs` in `src/extractors/`.
- Land DOM-tree integration (Track D dependency).
- Land `Trek::reextract_html` re-entrancy seam.

**Phase 2A — Social-and-conversational** (most fixture-relevant)
- `_conversation.rs` (already in P0)
- `chatgpt.rs`, `claude.rs`, `grok.rs`, `gemini.rs` (4 ai-chat)
- Pairs naturally because they share `_conversation` and produce identical output shape.

**Phase 2B — Social timelines** (largest single LOC)
- `twitter.rs`, `x_article.rs`, `x_oembed.rs`, `reddit.rs`, `threads.rs`, `bluesky.rs`, `mastodon.rs`, `linkedin.rs`, `discourse.rs`
- One agent, one family. ~3000 LOC. The X family is internally interlocked (registration order matters — see §4).

**Phase 2C — News + Knowledge + Dev** (independent extractors, similar shape)
- `hackernews.rs`, `lwn.rs`, `nytimes.rs`, `wikipedia.rs`, `medium.rs`, `substack.rs`, `c2_wiki.rs`, `github.rs`, `leetcode.rs`
- ~1750 LOC of mostly independent files.

**Phase 2D — Other**
- `youtube.rs` (1266 LOC, async-heavy InnerTube call)
- `bbcode_data.rs` (58 LOC; needs `bbcode.rs` util port)

---

## 4. Registry wiring — `extractors/mod.rs`

Default registration order **must match** the table below to preserve precedence semantics. The ordering rule from Defuddle:
- More specific URL patterns win when several extractors share a domain.
- Sync extractors come before async-only ones for the same domain.
- The catch-all (BBCode) is registered **last**.

### Recommended `extractors/mod.rs`

```rust
//! Site-specific extractors module.
//!
//! Registration order is significant — earlier entries win precedence ties
//! when their URL patterns overlap (X-Article > Twitter > X-Oembed for x.com).

use crate::extractor::{Extractor, ExtractorRegistry};

mod _base;
mod _conversation;
pub use _base::{BaseExtractor, ExtractorOptions, ExtractorResult, ExtractorVariables};
pub use _conversation::{ConversationExtractor, ConversationMessage, ConversationMetadata};

mod x_article;
mod twitter;
mod x_oembed;
mod reddit;
mod youtube;
mod hackernews;
mod chatgpt;
mod claude;
mod grok;
mod gemini;
mod github;
mod linkedin;
mod threads;
mod bluesky;
mod medium;
mod c2_wiki;
mod substack;
mod nytimes;
mod wikipedia;
mod mastodon;
mod discourse;
mod leetcode;
mod lwn;
mod bbcode_data;

pub fn register_default(reg: &mut ExtractorRegistry) {
    // Order matters — see module doc.
    reg.register(Box::new(x_article::XArticleExtractor::new()));
    reg.register(Box::new(twitter::TwitterExtractor::new()));
    reg.register(Box::new(x_oembed::XOembedExtractor::new()));
    reg.register(Box::new(reddit::RedditExtractor::new()));
    reg.register(Box::new(youtube::YoutubeExtractor::new()));
    reg.register(Box::new(hackernews::HackerNewsExtractor::new()));
    reg.register(Box::new(chatgpt::ChatGPTExtractor::new()));
    reg.register(Box::new(claude::ClaudeExtractor::new()));
    reg.register(Box::new(grok::GrokExtractor::new()));
    reg.register(Box::new(gemini::GeminiExtractor::new()));
    reg.register(Box::new(github::GitHubExtractor::new()));
    reg.register(Box::new(linkedin::LinkedInExtractor::new()));
    reg.register(Box::new(threads::ThreadsExtractor::new()));
    reg.register(Box::new(bluesky::BlueskyExtractor::new()));
    reg.register(Box::new(medium::MediumExtractor::new()));
    reg.register(Box::new(c2_wiki::C2WikiExtractor::new()));
    reg.register(Box::new(substack::SubstackExtractor::new()));
    reg.register(Box::new(nytimes::NytimesExtractor::new()));
    reg.register(Box::new(wikipedia::WikipediaExtractor::new()));
    reg.register(Box::new(mastodon::MastodonExtractor::new()));   // path-pattern
    reg.register(Box::new(discourse::DiscourseExtractor::new())); // path-pattern
    reg.register(Box::new(leetcode::LeetCodeExtractor::new()));
    reg.register(Box::new(lwn::LwnExtractor::new()));
    reg.register(Box::new(bbcode_data::BbcodeDataExtractor::new())); // catch-all, last
}
```

Order-of-precedence rules to encode in tests:
1. `x.com/<user>/article/<id>` → X-Article (not Twitter).
2. `x.com/<user>/status/<id>` → Twitter, falls back to X-Oembed only if `canExtract`/`canExtractAsync` returns true on the latter and not the former.
3. `*.reddit.com` → Reddit (regex domain match).
4. Mastodon and Discourse are path-pattern only — they apply across many self-hosted domains. Their `can_extract` must include the deep DOM probe (Defuddle does this too) so they don't false-match arbitrary `/@u/123` URLs.
5. BBCode is `/.*/` URL but DOM-gated; never matches without `#application_config[data-partnereventstore]`.

---

## 5. Per-extractor Rust LOC estimate

Rough multiplier from Defuddle TS LOC: **~0.9–1.3×** for direct ports (Rust is more verbose for type plumbing but lol_html/kuchikiki call sites are tighter than `el.querySelectorAll(...).forEach(...)`). Add ~30–80 LOC per file for tests.

| Extractor | TS LOC | Est. Rust LOC | Notes |
|---|---:|---:|---|
| BbcodeData | 58 | 80 | Plus ~250 LOC for `bbcode.rs` util port. |
| LeetCode | 23 | 40 | Trivial. |
| Wikipedia | 24 | 40 | Trivial. |
| Claude | 99 | 130 | Smallest conversation extractor. |
| Gemini | 130 | 175 | |
| LWN | 135 | 180 | |
| Medium | 140 | 190 | Needs role=button unwrap helper. |
| ChatGPT | 158 | 220 | Citation regex rewriting. |
| C2Wiki | 164 | 220 | Async-only, needs fetch. |
| Grok | 164 | 220 | Brittle utility-class selectors. |
| Discourse | 172 | 230 | |
| Substack | 211 | 290 | JSON `_preloads` parsing. |
| Reddit | 231 | 320 | Async old.reddit fetch. |
| LinkedIn | 257 | 350 | Many strip/unwrap passes. |
| Mastodon | 280 | 380 | Self-reply classification. |
| GitHub | 293 | 400 | Issue + PR + review-comment branches. |
| NYTimes | 294 | 410 | JSON content-block renderer. |
| HackerNews | 296 | 410 | Listing + story + comment trees. |
| X-Article | 328 | 450 | |
| Twitter | 332 | 460 | cellInnerDiv classifier. |
| Bluesky | 334 | 460 | Connector-line classifier. |
| Threads | 517 | 700 | Pagelet + region fallback. |
| X-Oembed | 531 | 720 | Async FxTwitter + oEmbed fallback. |
| YouTube | 1266 | 1400 | InnerTube client + caption parsing + sentence segmentation. |

Total Rust LOC estimate: **~8,475** across 25 extractors + ~600 LOC for `_base`/`_conversation`/`bbcode`/registry.

---

## 6. Fixture priority

`tests/fixtures/` filenames map directly to extractor coverage. Highest-value fixtures (one or more per extractor):

**Tier 1 — "must pass" before merging the family:**
- `general--x.com-article.html`, `general--x.com-article-2026-02-13.html` → X-Article
- `issues--161-x-status-url-author.html` → Twitter
- `comments--old.reddit.com-r-test-comments-abc123-test_post.html` → Reddit
- `general--news.ycombinator.com-item-id=12345678.html`, `comments--news.ycombinator.com-item-id=12345678.html`, `listing--news.ycombinator.com-news.html` → HackerNews
- `codeblocks--chatgpt-codemirror.html` → ChatGPT (also Track D code)
- `general--github.com-issue-56.html`, `general--github.com-test-owner-test-repo-pull-42.html` → GitHub
- `general--substack-app.html`, `general--substack-custom-domain.html`, `general--substack-note.html`, `general--substack-note-permalink.html` → Substack
- `general--wikipedia.html`, `general--wikipedia-ipa-pronunciation.html` → Wikipedia
- `comments--mastodon.social-@user-12345678.html` → Mastodon
- `extractor--bbcode-data.html` → BbcodeData
- `general--scp-wiki.wikidot.com-scp-9935.html` → (general fallback, validates BbcodeData *doesn't* eat it)

**Tier 2 — coverage gaps (no current fixtures; need synthetic):**
- Claude, Grok, Gemini conversation pages
- Threads, Bluesky, LinkedIn posts
- LWN article + comments
- NYTimes article (JSON)
- Discourse topic
- LeetCode problem
- Medium article
- C2Wiki page
- YouTube watch page (with caption response stubbed)

Recommendation: capture Tier 2 fixtures incrementally as each extractor is implemented — record one real-world page per site under `tests/fixtures/extractors--<name>--<slug>.html`. Lock the registered output as a snapshot via `insta` to catch regressions.

---

## 7. Concrete file list

**Phase 0 (blocking everyone):**
- `src/extractor.rs` — rewrite trait + registry + `ExtractCtx` + `ExtractorOptions`. Currently 146 LOC → ~250 LOC.
- `src/extractors/mod.rs` — replace 1-line stub with the registration block in §4.
- `src/extractors/_base.rs` — `BaseExtractor` with shared helpers (`postTitle`, `fetch` accessor). ~80 LOC.
- `src/extractors/_conversation.rs` — `ConversationExtractor` trait + `extract_conversation` free fn. ~150 LOC.
- `src/utils/bbcode.rs` — port `defuddle/utils/bbcode.ts` (precondition for BbcodeData). ~250 LOC.

**Phase 2A — ai-chat family (4 files, ~745 LOC):**
- `src/extractors/chatgpt.rs`
- `src/extractors/claude.rs`
- `src/extractors/grok.rs`
- `src/extractors/gemini.rs`

**Phase 2B — social family (9 files, ~3970 LOC):**
- `src/extractors/x_article.rs`
- `src/extractors/twitter.rs`
- `src/extractors/x_oembed.rs`
- `src/extractors/reddit.rs`
- `src/extractors/threads.rs`
- `src/extractors/bluesky.rs`
- `src/extractors/mastodon.rs`
- `src/extractors/linkedin.rs`
- `src/extractors/discourse.rs`

**Phase 2C — news + knowledge + dev (9 files, ~2200 LOC):**
- `src/extractors/hackernews.rs`
- `src/extractors/lwn.rs`
- `src/extractors/nytimes.rs`
- `src/extractors/wikipedia.rs`
- `src/extractors/medium.rs`
- `src/extractors/substack.rs`
- `src/extractors/c2_wiki.rs`
- `src/extractors/github.rs`
- `src/extractors/leetcode.rs`

**Phase 2D — other (2 files + 1 util, ~1480 LOC):**
- `src/extractors/youtube.rs`
- `src/extractors/bbcode_data.rs`

**Test scaffolding:**
- `tests/extractors/` — one integration test per family; uses fixtures table from §6.
- `tests/snapshots/` — `insta` snapshots committed alongside fixtures.

**Cargo additions:**
- `kuchikiki = "0.8"` — DOM library (shared with Track D).
- `insta = "1.40"` (dev) — snapshot tests.
- `futures = "0.3"` — `BoxFuture` for async extractor trait.
- Optional: `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }` for native `Fetcher` impl. WASM target uses a `web-sys`-based fetcher; gate with `cfg(target_arch = "wasm32")`.

---

## 8. Open questions

1. **Fetcher abstraction**: Defuddle accepts a custom `fetch` in `ExtractorOptions`. Trek needs an analogous `Fetcher` trait so library consumers can swap impls (worker vs. browser vs. CLI). Recommend a small `async fn fetch(&self, req: FetchRequest) -> Result<FetchResponse>` trait. Required by Reddit, X-Oembed, C2Wiki, YouTube.
2. **Re-entrancy**: `_conversation.ts` re-runs Defuddle on synthesized HTML. Trek must expose `Trek::parse_html_internal` (or equivalent) without the WASM bindings layer. Land in Phase 0.
3. **DOM crate**: must be the same as Track D. `kuchikiki` is the recommendation; lock it in shared types before either track starts.
4. **Async on WASM**: today Trek's `wasm.rs` is sync. Async extractors (XOembed, YouTube, Reddit, C2Wiki) need `wasm-bindgen-futures` glue — `wasm-bindgen-futures` is already in `Cargo.toml`, but the public `Trek` API needs a `parse_async` method; document this as a v0.3 API addition.
5. **Schema.org data**: trek currently extracts schema.org during the streaming pass; Defuddle uses it in `BaseExtractor` only optionally. Pass it through `ExtractCtx.schema_org_data` exactly as today — no change needed beyond the trait rewrite.
