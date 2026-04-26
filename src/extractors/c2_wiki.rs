//! C2 Wiki extractor — port of Defuddle's `c2-wiki.ts`.
//!
//! `wiki.c2.com` (and `c2.com/wiki`) serves no useful HTML body for an
//! article — the actual page text lives at
//! `https://c2.com/wiki/remodel/pages/<PageName>`. This extractor is async
//! by nature; on the sync path it returns a minimal stub.
// AGENT-P2C: Phase 2C knowledge extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};

/// C2 Wiki (`c2.com`, `wiki.c2.com`) extractor.
pub struct C2WikiExtractor;

impl C2WikiExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for C2WikiExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn url_is_c2(url: &str) -> bool {
    let Ok(p) = url::Url::parse(url) else {
        return false;
    };
    let Some(h) = p.host_str() else { return false };
    h == "wiki.c2.com" || h == "c2.com" || h.ends_with(".c2.com")
}

fn page_title_from_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    // querystring like `?WelcomeVisitors`
    let q = parsed.query()?;
    // First key — match `[A-Za-z]\w*`.
    let re = regex::Regex::new(r"^([A-Za-z]\w*)").ok()?;
    let cap = re.captures(q)?;
    Some(cap.get(1)?.as_str().to_string())
}

fn split_camel_case(s: &str) -> String {
    // "WelcomeVisitors" -> "Welcome Visitors"
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && c.is_ascii_uppercase() {
            // peek prev — only insert space if prev was lowercase.
            let prev = s.chars().nth(i - 1).unwrap_or(' ');
            if prev.is_ascii_lowercase() {
                out.push(' ');
            }
        }
        out.push(c);
    }
    out
}

impl Extractor for C2WikiExtractor {
    fn name(&self) -> &'static str {
        "c2_wiki"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        // Sync path: only when fetcher present (we still report not-runnable
        // here so the host falls through gracefully to generic extraction
        // when there's no async harness).
        if !ctx.url.is_some_and(url_is_c2) {
            return false;
        }
        ctx.fetcher.is_some()
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        _root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let url = ctx.url.unwrap_or_default();
        let title = page_title_from_url(url).unwrap_or_else(|| "WelcomeVisitors".to_string());
        let pretty = split_camel_case(&title);
        // Without a fetcher we degrade to a metadata-only stub — the host's
        // generic pipeline will run on the actual DOM next.
        Err(ExtractError::Failed {
            name: "c2_wiki",
            reason: format!("c2_wiki extraction requires async fetch of {pretty} ({title})",),
        })
    }

    fn prefers_async(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn url_match() {
        let e = C2WikiExtractor::new();
        // No fetcher → can_extract returns false.
        let ctx = ExtractCtx::new(Some("https://wiki.c2.com/?WelcomeVisitors"), &[]);
        assert!(!e.can_extract(&ctx));
    }

    #[test]
    fn camel_case_split() {
        assert_eq!(split_camel_case("WelcomeVisitors"), "Welcome Visitors");
        assert_eq!(split_camel_case("SomeWikiPage"), "Some Wiki Page");
    }

    #[test]
    fn title_from_url_works() {
        assert_eq!(
            page_title_from_url("https://wiki.c2.com/?WelcomeVisitors"),
            Some("WelcomeVisitors".to_string())
        );
        assert_eq!(page_title_from_url("https://wiki.c2.com/"), None);
    }

    #[test]
    fn extract_returns_failed_without_fetcher() {
        let e = C2WikiExtractor::new();
        let ctx = ExtractCtx::new(Some("https://wiki.c2.com/?WelcomeVisitors"), &[]);
        let root = parse_html("<html></html>");
        assert!(e.extract(&ctx, &root).is_err());
    }
}
