// AGENT-P2B: X-Oembed extractor — async-only fallback that fetches
// FxTwitter / publish.twitter.com oEmbed and renders the response.
//
// Port of `defuddle/src/extractors/x-oembed.ts`. In Trek's current sync
// pipeline this extractor reports `prefers_async = true` and refuses to
// extract — it only kicks in once the async parse path is wired and a
// `Fetcher` is available. With no fetcher (`ctx.fetcher = None`) the
// extractor must return `ExtractError::Failed` so the host falls back to
// generic extraction.

use kuchikiki::NodeRef;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};

/// AGENT-P2B: matches both `/status/<id>` and `/article/<id>` paths so the
/// async fetch can target either FxTwitter endpoint. Defuddle does the same
/// `/(status|article)/\d+/` test in `canExtractAsync`.
static X_OEMBED_URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^https?://(?:www\.|mobile\.)?(?:x|twitter)\.com/[A-Za-z0-9_]{1,15}/(?:status|article)/\d+")
        .expect("valid regex")
});

/// X-Oembed extractor — async-only.
pub struct XOembedExtractor;

impl XOembedExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn url_match(url: Option<&str>) -> bool {
        url.is_some_and(|u| X_OEMBED_URL.is_match(u))
    }
}

impl Default for XOembedExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for XOembedExtractor {
    fn name(&self) -> &'static str {
        "x-oembed"
    }

    fn prefers_async(&self) -> bool {
        // Mirrors Defuddle: `canExtract` always returns false; the async
        // path is the only way this extractor produces output.
        true
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        // Only the async-aware registry should pick this; the sync
        // `select` filters out `prefers_async` already, so this is a
        // belt-and-braces guard.
        Self::url_match(ctx.url)
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        _root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        // The sync path has no FxTwitter / oEmbed fetch; surface a
        // `Failed` so the host pipeline keeps the generic-extraction
        // fallback chain intact. The async fetch is wired in a later
        // phase that adds `Fetcher` plumbing to the sync trait surface.
        Err(ExtractError::Failed {
            name: "x-oembed",
            reason: "no fetcher available — async path not yet wired".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuchikiki::traits::TendrilSink;

    fn parse(html: &str) -> NodeRef {
        kuchikiki::parse_html().one(html)
    }

    #[test]
    fn url_match_accepts_status_and_article() {
        let e = XOembedExtractor::new();
        let ctx = ExtractCtx::new(Some("https://x.com/jane/status/12345"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx = ExtractCtx::new(Some("https://twitter.com/jane/article/9"), &[]);
        assert!(e.can_extract(&ctx));
    }

    #[test]
    fn url_match_rejects_non_x() {
        let e = XOembedExtractor::new();
        let ctx = ExtractCtx::new(Some("https://example.com/jane/status/1"), &[]);
        assert!(!e.can_extract(&ctx));
    }

    #[test]
    fn extract_without_fetcher_fails() {
        let e = XOembedExtractor::new();
        assert!(e.prefers_async());
        let root = parse("<html><body></body></html>");
        let ctx = ExtractCtx::new(Some("https://x.com/jane/status/1"), &[]);
        let err = e.extract(&ctx, &root).unwrap_err();
        match err {
            ExtractError::Failed { name, .. } => assert_eq!(name, "x-oembed"),
            other => panic!("expected Failed, got {other:?}"),
        }
    }
}
