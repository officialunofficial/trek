#![allow(
    clippy::doc_markdown,            // many proper nouns (FxTwitter, ChatGPT, X-Oembed) flagged falsely
    clippy::too_long_first_doc_paragraph
)]
//! Site-specific content extractors — trait surface, registry, and supporting types.
//!
//! This file is the Phase-0 foundation that lets Round-3 agents port Defuddle's
//! 25 site extractors in parallel without touching shared infrastructure. It
//! provides:
//!
//! - The [`Extractor`] trait — what every site extractor implements.
//! - [`ExtractCtx`] — the read-only context passed to every extractor (URL,
//!   schema.org data, optional async [`Fetcher`], debug flag, recursion depth).
//! - [`ExtractedContent`] — what an extractor returns. Fields set to `None`
//!   tell the host pipeline to fall back to generic-extracted metadata.
//! - [`ConversationExtractor`] — companion trait for chat-style extractors
//!   (Reddit, HN, Mastodon, ChatGPT/Claude/Gemini/Grok).
//! - [`Fetcher`] — async HTTP indirection so Reddit/X-Oembed/YouTube/C2-Wiki
//!   can fetch supplementary URLs without binding the library to reqwest.
//! - [`ExtractorRegistry`] — priority-ordered list with `select(ctx)`.
//! - [`GenericExtractor`] — the always-last fallback that defers to the host
//!   pipeline.
//!
//! Phase-0 ships an *empty default registry* so nothing site-specific runs
//! yet. Round-3 agents will register the actual extractors in
//! `src/extractors/mod.rs::ExtractorRegistry::with_defaults`.

use crate::types::ExtractedContent as LegacyExtractedContent;
use async_trait::async_trait;
use kuchikiki::NodeRef;
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, instrument};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors a site extractor can return from `extract`.
#[derive(Debug, Error)]
pub enum ExtractError {
    /// The extractor matched the URL/DOM but couldn't actually pull
    /// content (e.g. JSON shape changed). The host pipeline should fall
    /// back to the generic path.
    #[error("extractor `{name}` could not extract: {reason}")]
    Failed {
        /// Extractor name.
        name: &'static str,
        /// Human-readable reason.
        reason: String,
    },

    /// A required supplementary fetch failed.
    #[error("fetcher error: {0}")]
    Fetch(#[from] FetchError),

    /// The DOM was malformed in a way the extractor couldn't recover from.
    #[error("dom error: {0}")]
    Dom(String),

    /// Recursion limit hit while expanding nested HTML (e.g. X-Article
    /// unwrapping a quote-tweet that quotes another tweet, ad infinitum).
    #[error("recursion depth exceeded (max {max})")]
    RecursionLimit {
        /// Configured cap.
        max: u32,
    },

    /// Anything else.
    #[error("extractor error: {0}")]
    Other(String),
}

/// Errors a [`Fetcher`] can return.
#[derive(Debug, Error)]
pub enum FetchError {
    /// The current build/host doesn't support outbound fetches at all.
    /// Returned by [`NoOpFetcher`]; expected when running synchronous
    /// extraction without a host-provided fetcher.
    #[error("fetch unsupported in this environment")]
    Unsupported,

    /// The URL wasn't well-formed.
    #[error("invalid url: {0}")]
    InvalidUrl(String),

    /// Underlying transport / IO error.
    #[error("transport error: {0}")]
    Transport(String),

    /// Got a non-2xx HTTP response.
    #[error("http {status} from {url}")]
    Http {
        /// HTTP status code.
        status: u16,
        /// Requested URL.
        url: String,
    },
}

// ---------------------------------------------------------------------------
// Recursion guard
// ---------------------------------------------------------------------------

/// Tracks recursive [`crate::Trek::parse_html_internal`] calls so an extractor
/// that re-feeds embedded HTML through the pipeline (X-Article, quote-tweets,
/// nested conversation messages) can't infinite-loop on malicious or
/// pathologically nested input.
///
/// The cap is shared across the entire `Trek::parse` invocation and travels
/// inside the [`ExtractCtx`] passed to extractors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecursionDepth {
    current: u32,
    max: u32,
}

impl RecursionDepth {
    /// Default cap — three nested re-entrancy calls is more than enough for
    /// any real-world site (X-Article quoting a tweet quoting an X-Article
    /// is the deepest known case, depth 2).
    pub const DEFAULT_MAX: u32 = 3;

    /// Construct a fresh counter at depth 0 with the default cap.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            current: 0,
            max: Self::DEFAULT_MAX,
        }
    }

    /// Construct with an explicit cap (testing).
    #[must_use]
    pub const fn with_max(max: u32) -> Self {
        Self { current: 0, max }
    }

    /// Current depth.
    #[must_use]
    pub const fn current(&self) -> u32 {
        self.current
    }

    /// Configured maximum.
    #[must_use]
    pub const fn max(&self) -> u32 {
        self.max
    }

    /// Returns a new counter with `current + 1`, or
    /// [`ExtractError::RecursionLimit`] if that would exceed `max`.
    #[allow(clippy::missing_const_for_fn)] // ExtractError::RecursionLimit isn't const-constructible
    pub fn enter(self) -> Result<Self, ExtractError> {
        if self.current >= self.max {
            Err(ExtractError::RecursionLimit { max: self.max })
        } else {
            Ok(Self {
                current: self.current + 1,
                max: self.max,
            })
        }
    }
}

impl Default for RecursionDepth {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Fetcher
// ---------------------------------------------------------------------------

/// Async HTTP indirection used by extractors that need supplementary fetches
/// (Reddit comments JSON, X-Oembed, C2-Wiki source, YouTube transcript).
///
/// Trek is library code and intentionally agnostic about *how* HTTP is
/// performed. Native callers can opt into [`ReqwestFetcher`] by enabling the
/// `fetcher-reqwest` Cargo feature. WASM callers (the Worker, the browser
/// playground) provide their own JS-backed fetcher via the WASM bindings
/// — see Track F for the shim.
///
/// Synchronous callers can pass [`NoOpFetcher`], which makes any extractor
/// that requires a fetch fail with [`FetchError::Unsupported`]. Such
/// extractors should also return `prefers_async = true` so the registry
/// knows to skip them on the sync path.
#[async_trait]
pub trait Fetcher: Send + Sync {
    /// Perform a `GET` against `url` and return the body as a string.
    async fn fetch(&self, url: &str) -> Result<String, FetchError>;
}

/// A [`Fetcher`] that always returns [`FetchError::Unsupported`]. Use it as a
/// placeholder when no real fetcher is available (e.g. synchronous CLI
/// invocation).
pub struct NoOpFetcher;

#[async_trait]
impl Fetcher for NoOpFetcher {
    async fn fetch(&self, _url: &str) -> Result<String, FetchError> {
        Err(FetchError::Unsupported)
    }
}

// Native-only `reqwest`-backed fetcher. Gated on the `fetcher-reqwest`
// feature so WASM builds don't pull in reqwest (which doesn't compile to
// `wasm32-unknown-unknown` with rustls).
#[cfg(all(feature = "fetcher-reqwest", not(target_arch = "wasm32")))]
mod reqwest_impl {
    use super::{FetchError, Fetcher};
    use async_trait::async_trait;

    /// A [`Fetcher`] backed by `reqwest`. Available only on native targets
    /// when the `fetcher-reqwest` feature is enabled.
    pub struct ReqwestFetcher {
        client: reqwest::Client,
    }

    impl ReqwestFetcher {
        /// Construct with a default reqwest client.
        #[must_use]
        pub fn new() -> Self {
            Self {
                client: reqwest::Client::new(),
            }
        }

        /// Construct with a caller-supplied client.
        #[must_use]
        pub const fn from_client(client: reqwest::Client) -> Self {
            Self { client }
        }
    }

    impl Default for ReqwestFetcher {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl Fetcher for ReqwestFetcher {
        async fn fetch(&self, url: &str) -> Result<String, FetchError> {
            let resp = self
                .client
                .get(url)
                .send()
                .await
                .map_err(|e| FetchError::Transport(e.to_string()))?;
            let status = resp.status();
            if !status.is_success() {
                return Err(FetchError::Http {
                    status: status.as_u16(),
                    url: url.to_string(),
                });
            }
            resp.text()
                .await
                .map_err(|e| FetchError::Transport(e.to_string()))
        }
    }
}

#[cfg(all(feature = "fetcher-reqwest", not(target_arch = "wasm32")))]
pub use reqwest_impl::ReqwestFetcher;

// ---------------------------------------------------------------------------
// ExtractCtx + ExtractedContent
// ---------------------------------------------------------------------------

/// Read-only context handed to every [`Extractor::can_extract`] and
/// [`Extractor::extract`] call.
///
/// `ExtractCtx` borrows from the caller; extractors must not retain it past
/// the function return. Trek constructs one per `parse` invocation.
pub struct ExtractCtx<'a> {
    /// The page URL, if known. Many extractors gate on this (regex match
    /// against `x.com`, `*.reddit.com`, etc.).
    pub url: Option<&'a str>,

    /// Schema.org `@graph` blocks already parsed by the streaming first
    /// pass. Cheap to scan; extractors should prefer this over re-parsing.
    pub schema_org: &'a [Value],

    /// Optional async fetcher. `None` means "synchronous host"; extractors
    /// that hard-require a fetch should return `prefers_async = true` and
    /// gracefully fail on the sync path.
    pub fetcher: Option<&'a dyn Fetcher>,

    /// Whether the host opted into verbose diagnostic logging.
    pub debug: bool,

    /// Recursion guard for `parse_html_internal` re-entrancy. See
    /// [`RecursionDepth`].
    pub recursion: RecursionDepth,
}

impl<'a> ExtractCtx<'a> {
    /// Construct a new context. Most callers only need URL and schema.
    #[must_use]
    pub const fn new(url: Option<&'a str>, schema_org: &'a [Value]) -> Self {
        Self {
            url,
            schema_org,
            fetcher: None,
            debug: false,
            recursion: RecursionDepth::new(),
        }
    }

    /// Builder-style setter for the fetcher.
    #[must_use]
    pub const fn with_fetcher(mut self, fetcher: &'a dyn Fetcher) -> Self {
        self.fetcher = Some(fetcher);
        self
    }

    /// Builder-style setter for the debug flag.
    #[must_use]
    pub const fn with_debug(mut self, debug: bool) -> Self {
        self.debug = debug;
        self
    }

    /// Builder-style setter for the recursion counter.
    #[must_use]
    pub const fn with_recursion(mut self, recursion: RecursionDepth) -> Self {
        self.recursion = recursion;
        self
    }

    /// Returns a child context with `recursion.current + 1`. Used by
    /// extractors before calling back into [`crate::Trek::parse_html_internal`].
    pub fn enter_recursion(&self) -> Result<Self, ExtractError> {
        Ok(Self {
            url: self.url,
            schema_org: self.schema_org,
            fetcher: self.fetcher,
            debug: self.debug,
            recursion: self.recursion.enter()?,
        })
    }
}

/// What an extractor returns to the host pipeline.
///
/// All metadata fields are `Option<String>`. **`None` means "fall back to
/// generic metadata"** — the host pipeline merges these on top of the
/// streaming-extracted defaults so extractors only need to override what
/// they actually know better.
#[derive(Debug, Clone, Default)]
pub struct ExtractedContent {
    /// Cleaned HTML body that will become the `content` field of
    /// [`crate::TrekResponse`].
    pub content_html: String,

    /// Optional title override.
    pub title: Option<String>,

    /// Optional author override.
    pub author: Option<String>,

    /// Optional site / publication name override.
    pub site: Option<String>,

    /// Optional ISO-8601 published date override.
    pub published: Option<String>,

    /// Optional description override.
    pub description: Option<String>,

    /// Additional schema.org blocks the extractor synthesized (e.g. the
    /// fake `Conversation` blob conversation extractors emit). Merged onto
    /// the streaming-collected schema in the final response.
    pub schema_overrides: Vec<Value>,
}

impl ExtractedContent {
    /// Convert into the legacy `ExtractedContent` shape used by
    /// [`crate::Trek::parse`] today. This shim lets us land Phase-0 without
    /// rewriting the full pipeline in lib.rs.
    #[must_use]
    pub fn into_legacy(self) -> LegacyExtractedContent {
        LegacyExtractedContent {
            title: self.title,
            author: self.author,
            published: self.published,
            content: None,
            content_html: Some(self.content_html),
            variables: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Extractor trait
// ---------------------------------------------------------------------------

/// A site-specific content extractor.
///
/// Implementors are registered with [`ExtractorRegistry`] in priority order.
/// The host pipeline (`Trek::parse`) walks the registry, calls
/// [`Extractor::can_extract`] on each, and uses the first match's
/// [`Extractor::extract`] output. If no extractor matches, the host falls
/// back to generic extraction.
///
/// `extract` is *synchronous*. Extractors that need to perform supplementary
/// HTTP requests do so by reading [`ExtractCtx::fetcher`] and either:
///
/// - Returning `prefers_async = true` so the registry only picks them on the
///   async path, or
/// - Falling back to a degraded sync result and letting the async path do
///   the richer extraction later.
///
/// The split keeps the synchronous WASM API simple: today's `TrekWasm.parse`
/// can keep being a sync call; Round-3 async wiring is a separate concern.
pub trait Extractor: Send + Sync {
    /// Extractor name for diagnostics, telemetry, and the
    /// `extractor_type` field in `TrekResponse`.
    fn name(&self) -> &'static str;

    /// Return true if this extractor wants to handle the document. Should
    /// be cheap — a URL regex test plus maybe a small DOM probe via
    /// `kuchikiki`. Heavy work belongs in [`Self::extract`].
    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool;

    /// Pull structured content from the parsed DOM tree.
    ///
    /// `root` is the kuchikiki document root that the host parsed once,
    /// up-front. Extractors must not mutate it.
    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError>;

    /// Whether this extractor needs network IO and therefore only really
    /// works on the async path. Defaults to `false`.
    ///
    /// Used by the registry's async-aware selection helpers; sync callers
    /// will skip extractors with this set unless the same struct also has a
    /// reasonable sync degradation.
    fn prefers_async(&self) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// ConversationExtractor companion trait
// ---------------------------------------------------------------------------

/// One message in a conversation thread (Reddit comment, HN reply, ChatGPT
/// turn, Mastodon toot in a self-reply chain).
#[derive(Debug, Clone, Default)]
pub struct ConversationMessage {
    /// Display name / role (`"User"`, `"Assistant"`, a Reddit username).
    pub author: Option<String>,

    /// Optional ISO-8601 timestamp.
    pub timestamp: Option<String>,

    /// HTML body of the message. May contain block elements.
    pub html: String,

    /// Indentation depth in the thread tree. `0` = top-level.
    pub depth: u32,
}

impl ConversationMessage {
    /// Render this message as a depth-indented blockquote with author /
    /// timestamp header. Mirrors Defuddle's `_conversation.ts` output
    /// closely enough that downstream markdown conversion produces the
    /// same shape.
    #[must_use]
    pub fn render_html(&self) -> String {
        let mut out = String::new();
        for _ in 0..self.depth {
            out.push_str("<blockquote>");
        }
        out.push_str("<div class=\"conversation-message\">");
        if let Some(author) = &self.author {
            out.push_str("<p class=\"conversation-author\"><strong>");
            out.push_str(&html_escape::encode_text(author));
            out.push_str("</strong></p>");
        }
        if let Some(ts) = &self.timestamp {
            out.push_str("<p class=\"conversation-timestamp\"><em>");
            out.push_str(&html_escape::encode_text(ts));
            out.push_str("</em></p>");
        }
        out.push_str(&self.html);
        out.push_str("</div>");
        for _ in 0..self.depth {
            out.push_str("</blockquote>");
        }
        out
    }
}

/// Companion trait for chat-style extractors (Reddit, HN, ChatGPT, Claude,
/// Gemini, Grok, Mastodon, etc.).
///
/// Implementors return a flat list of [`ConversationMessage`]s; their own
/// [`Extractor::extract`] usually delegates to [`render_conversation`] to
/// produce the final HTML. Defuddle does roughly the same and then re-runs
/// its own pipeline on the synthesized HTML — Trek does that too via
/// [`crate::Trek::parse_html_internal`].
pub trait ConversationExtractor: Extractor {
    /// Pull the messages from the DOM. Order matters: the slice is
    /// rendered top-to-bottom in the final HTML.
    fn extract_conversation(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError>;
}

/// Render a conversation thread as a single HTML string. Free function so
/// implementors of [`ConversationExtractor`] can call it from their own
/// `extract` impls without trait-object plumbing.
#[must_use]
pub fn render_conversation(messages: &[ConversationMessage]) -> String {
    let mut out = String::new();
    out.push_str("<article class=\"conversation\">");
    for msg in messages {
        out.push_str(&msg.render_html());
    }
    out.push_str("</article>");
    out
}

// ---------------------------------------------------------------------------
// ExtractorRegistry
// ---------------------------------------------------------------------------

/// Priority-ordered registry of [`Extractor`] implementations.
///
/// Order matters — see `src/extractors/mod.rs` for the canonical
/// registration sequence. The high-level rules are:
///
/// 1. **More specific URL patterns win.** `XArticleExtractor` is registered
///    *before* `TwitterExtractor` so a long-form X article isn't classified
///    as a tweet.
/// 2. **Sync extractors come before async-only ones for the same domain.**
///    `XOembedExtractor` (`prefers_async = true`) sits after the two sync
///    X extractors.
/// 3. **Deep DOM-gated path-pattern extractors before the generic
///    catch-all.** `Wikipedia` lives before any `/.*/`-style match.
/// 4. **`BBCodeDataExtractor` is registered last** — it matches every URL
///    and DOM-gates on `#application_config[data-partnereventstore]`.
///
/// Phase-0 ships with an *empty* default registry; Round-3 agents will
/// populate it via `with_defaults` in `src/extractors/mod.rs` as they port
/// each family.
pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn Extractor>>,
}

impl std::fmt::Debug for ExtractorRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractorRegistry")
            .field("count", &self.extractors.len())
            .field(
                "names",
                &self.extractors.iter().map(|e| e.name()).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorRegistry {
    /// Empty registry. Use [`Self::with_defaults`] to get the priority-ordered
    /// set Track-E Round-3 ships.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            extractors: Vec::new(),
        }
    }

    /// Construct the production registry. Phase-0 returns an empty
    /// registry; the Round-3 agents will fill this in as they port each
    /// extractor family. The `extractors/mod.rs` module owns the actual
    /// registration calls so changes there don't ripple into this file.
    #[must_use]
    pub fn with_defaults() -> Self {
        crate::extractors::register_defaults(Self::new())
    }

    /// Append an extractor. Order is preserved.
    pub fn register(&mut self, extractor: Box<dyn Extractor>) {
        debug!("registering extractor: {}", extractor.name());
        self.extractors.push(extractor);
    }

    /// Number of registered extractors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.extractors.len()
    }

    /// True if no extractors are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.extractors.is_empty()
    }

    /// Find the first extractor that wants this document. Skips extractors
    /// that prefer the async path.
    #[instrument(skip(self, ctx), fields(url = ?ctx.url))]
    pub fn select<'a>(&'a self, ctx: &ExtractCtx<'_>) -> Option<&'a dyn Extractor> {
        for e in &self.extractors {
            if e.prefers_async() {
                continue;
            }
            if e.can_extract(ctx) {
                debug!("selected extractor: {}", e.name());
                return Some(e.as_ref());
            }
        }
        None
    }

    /// Find the first async-preferred extractor that wants this document.
    /// Used by the (future) async parse path.
    #[instrument(skip(self, ctx), fields(url = ?ctx.url))]
    pub fn select_async<'a>(&'a self, ctx: &ExtractCtx<'_>) -> Option<&'a dyn Extractor> {
        for e in &self.extractors {
            if !e.prefers_async() {
                continue;
            }
            if e.can_extract(ctx) {
                debug!("selected async extractor: {}", e.name());
                return Some(e.as_ref());
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// GenericExtractor — the always-last fallback
// ---------------------------------------------------------------------------

/// Last-resort extractor that *never* claims a document. The host pipeline
/// recognises this and runs its own generic extraction path. Kept as a real
/// `Extractor` impl so the registry surface is uniform and future tracks
/// can swap in a real generic extractor without changing call sites.
pub struct GenericExtractor;

impl Extractor for GenericExtractor {
    fn name(&self) -> &'static str {
        "generic"
    }

    fn can_extract(&self, _ctx: &ExtractCtx<'_>) -> bool {
        // Never matches — host pipeline owns the generic path.
        false
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        _root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        Err(ExtractError::Failed {
            name: "generic",
            reason: "GenericExtractor::extract should never be called; \
                     can_extract always returns false"
                .to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // OK to use unwrap/expect in tests
mod tests {
    use super::*;
    use serde_json::json;

    fn parse_dom(html: &str) -> NodeRef {
        use kuchikiki::traits::TendrilSink;
        kuchikiki::parse_html().one(html)
    }

    /// 1. ExtractCtx construction — defaults, builder methods.
    #[test]
    fn extract_ctx_construction() {
        let schema: Vec<Value> = vec![json!({"@type": "Article"})];
        let ctx = ExtractCtx::new(Some("https://example.com"), &schema);
        assert_eq!(ctx.url, Some("https://example.com"));
        assert_eq!(ctx.schema_org.len(), 1);
        assert!(ctx.fetcher.is_none());
        assert!(!ctx.debug);
        assert_eq!(ctx.recursion.current(), 0);
        assert_eq!(ctx.recursion.max(), RecursionDepth::DEFAULT_MAX);

        let fetcher = NoOpFetcher;
        let ctx2 = ExtractCtx::new(None, &[])
            .with_fetcher(&fetcher)
            .with_debug(true);
        assert!(ctx2.fetcher.is_some());
        assert!(ctx2.debug);
    }

    /// 2. Registry selection priority — sync skips async-preferred entries.
    #[test]
    fn registry_selection_priority() {
        struct Always {
            name: &'static str,
            async_only: bool,
        }
        impl Extractor for Always {
            fn name(&self) -> &'static str {
                self.name
            }
            fn can_extract(&self, _ctx: &ExtractCtx<'_>) -> bool {
                true
            }
            fn extract(
                &self,
                _ctx: &ExtractCtx<'_>,
                _root: &NodeRef,
            ) -> Result<ExtractedContent, ExtractError> {
                Ok(ExtractedContent::default())
            }
            fn prefers_async(&self) -> bool {
                self.async_only
            }
        }

        let mut reg = ExtractorRegistry::new();
        // Async-preferred entry registered first must NOT be selected on
        // the sync path — order in the registry is preserved, but
        // prefers_async filters separately.
        reg.register(Box::new(Always {
            name: "async-first",
            async_only: true,
        }));
        reg.register(Box::new(Always {
            name: "sync-second",
            async_only: false,
        }));
        reg.register(Box::new(Always {
            name: "sync-third",
            async_only: false,
        }));

        let ctx = ExtractCtx::new(None, &[]);
        let picked = reg.select(&ctx).expect("should pick something");
        assert_eq!(
            picked.name(),
            "sync-second",
            "sync select must skip async-only entries even when registered first"
        );

        let picked_async = reg.select_async(&ctx).expect("async pick");
        assert_eq!(picked_async.name(), "async-first");
    }

    /// 3. NoOpFetcher returns Unsupported.
    #[test]
    fn noop_fetcher_unsupported() {
        // Hand-rolled poll loop avoids needing a runtime — NoOpFetcher's
        // future is ready immediately (no awaits inside).
        use std::future::Future;
        use std::pin::Pin;
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        const VTABLE: RawWakerVTable = RawWakerVTable::new(
            |_| RawWaker::new(std::ptr::null(), &VTABLE),
            |_| {},
            |_| {},
            |_| {},
        );
        let waker = unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) };
        let mut cx = Context::from_waker(&waker);

        let f = NoOpFetcher;
        let fut = f.fetch("https://example.com");
        let mut fut = Box::pin(fut);
        match Pin::new(&mut fut).poll(&mut cx) {
            Poll::Ready(Err(FetchError::Unsupported)) => {}
            Poll::Ready(other) => panic!("expected Unsupported, got {other:?}"),
            Poll::Pending => panic!("NoOpFetcher should resolve immediately"),
        }
    }

    /// 4. ConversationMessage rendering — depth-driven blockquote nesting.
    #[test]
    fn conversation_message_render() {
        let m = ConversationMessage {
            author: Some("alice".to_string()),
            timestamp: Some("2026-04-26T12:00:00Z".to_string()),
            html: "<p>hi</p>".to_string(),
            depth: 2,
        };
        let html = m.render_html();
        // Two opening blockquotes, two closing ones — preserves thread shape.
        assert_eq!(html.matches("<blockquote>").count(), 2);
        assert_eq!(html.matches("</blockquote>").count(), 2);
        assert!(html.contains("<strong>alice</strong>"));
        assert!(html.contains("<em>2026-04-26T12:00:00Z</em>"));
        assert!(html.contains("<p>hi</p>"));

        // The whole-thread renderer wraps in <article class="conversation">.
        let bundle = render_conversation(&[m.clone(), m]);
        assert!(bundle.starts_with("<article class=\"conversation\">"));
        assert!(bundle.ends_with("</article>"));
    }

    /// 5. RecursionDepth cap — enter() succeeds up to max, then errors.
    #[test]
    fn recursion_depth_cap() {
        let d = RecursionDepth::with_max(2);
        assert_eq!(d.current(), 0);
        let d1 = d.enter().expect("0->1");
        assert_eq!(d1.current(), 1);
        let d2 = d1.enter().expect("1->2");
        assert_eq!(d2.current(), 2);
        let err = d2.enter().expect_err("2->3 must fail at cap=2");
        assert!(matches!(err, ExtractError::RecursionLimit { max: 2 }));

        // Default cap is 3.
        let dd = RecursionDepth::new();
        assert_eq!(dd.max(), RecursionDepth::DEFAULT_MAX);
        let dd3 = dd.enter().unwrap().enter().unwrap().enter().unwrap();
        assert_eq!(dd3.current(), 3);
        assert!(dd3.enter().is_err());

        // ExtractCtx::enter_recursion threads through.
        let ctx = ExtractCtx::new(None, &[]).with_recursion(RecursionDepth::with_max(1));
        let ctx1 = ctx.enter_recursion().expect("first enter ok");
        assert_eq!(ctx1.recursion.current(), 1);
        assert!(ctx1.enter_recursion().is_err());
    }

    /// 6. ExtractedContent merging onto generic metadata — None preserves baseline.
    #[test]
    fn extracted_content_merges_onto_metadata() {
        let extracted = ExtractedContent {
            content_html: "<p>body</p>".to_string(),
            title: Some("Override Title".to_string()),
            author: None,
            site: Some("Example".to_string()),
            published: None,
            description: None,
            schema_overrides: vec![],
        };

        // Pretend baseline (what the streaming pass produced).
        let mut title = "Generic Title".to_string();
        let mut author = "Generic Author".to_string();
        let mut site = String::new();
        let mut published = "2026-01-01".to_string();

        if let Some(t) = &extracted.title {
            title = t.clone();
        }
        if let Some(a) = &extracted.author {
            author = a.clone();
        }
        if let Some(s) = &extracted.site {
            site = s.clone();
        }
        if let Some(p) = &extracted.published {
            published = p.clone();
        }

        // title was overridden, author preserved, site filled in,
        // published preserved (None falls through).
        assert_eq!(title, "Override Title");
        assert_eq!(author, "Generic Author");
        assert_eq!(site, "Example");
        assert_eq!(published, "2026-01-01");
    }

    /// 7. GenericExtractor never claims, returns Failed if forced.
    #[test]
    fn generic_extractor_never_matches() {
        let g = GenericExtractor;
        let ctx = ExtractCtx::new(Some("https://anything.test/"), &[]);
        assert!(!g.can_extract(&ctx));

        let root = parse_dom("<html><body><p>hi</p></body></html>");
        let err = g.extract(&ctx, &root).unwrap_err();
        match err {
            ExtractError::Failed { name, .. } => assert_eq!(name, "generic"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// 8. Registry can register the GenericExtractor and still never select it.
    #[test]
    fn registry_skips_generic() {
        let mut reg = ExtractorRegistry::new();
        reg.register(Box::new(GenericExtractor));
        let ctx = ExtractCtx::new(Some("https://example.com"), &[]);
        assert!(reg.select(&ctx).is_none());
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);
    }

    /// 9. with_defaults registers the Round-3 extractor suite (Defuddle parity).
    #[test]
    fn with_defaults_registers_round_3_extractors() {
        let reg = ExtractorRegistry::with_defaults();
        // 24 Round-3 extractors registered; updated by Round-4 agents as they
        // add or remove sites. Lower-bound assertion so cosmetic adds don't
        // break the test.
        assert!(!reg.is_empty());
        assert!(reg.len() >= 20);
    }
}
