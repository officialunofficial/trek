//! Wikipedia extractor — port of Defuddle's `wikipedia.ts`.
//!
//! Triggers on `*.wikipedia.org` URLs with `#mw-content-text` present.
//! Extracts the parser output, strips Wikipedia-specific scaffolding
//! (TOC, references, navbox, infobox), and normalizes the title (drops
//! the trailing `– Wikipedia` suffix).
// AGENT-P2C: Phase 2C knowledge extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{find_first, host_matches_suffix, remove_all, serialize_children};

/// Wikipedia (`*.wikipedia.org`) extractor.
pub struct WikipediaExtractor;

impl WikipediaExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for WikipediaExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn url_matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    host == "wikipedia.org" || host.ends_with(".wikipedia.org")
}

impl Extractor for WikipediaExtractor {
    fn name(&self) -> &'static str {
        "wikipedia"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url.is_some_and(url_matches)
            || ctx
                .url
                .is_some_and(|u| host_matches_suffix(u, "wikipedia.org"))
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let body =
            find_first(root, ".mw-parser-output").or_else(|| find_first(root, "#mw-content-text"));
        let body = body.ok_or_else(|| ExtractError::Failed {
            name: "wikipedia",
            reason: "no #mw-content-text".into(),
        })?;

        // Strip Wikipedia-specific scaffolding.
        for sel in &[
            ".toc",
            ".reflist",
            ".references",
            ".navbox",
            ".infobox",
            "#toc",
            ".mw-editsection",
            ".reference",
            ".mw-empty-elt",
            ".hatnote",
            ".mbox-small",
            ".sistersitebox",
            ".portalbox",
            ".thumbcaption .magnify",
        ] {
            remove_all(&body, sel);
        }

        let html = serialize_children(&body);

        let og_title = crate::extractors::meta_property(root, "og:title").unwrap_or_default();
        let title = strip_wikipedia_suffix(&og_title);
        let title = if title.is_empty() {
            find_first(root, "h1#firstHeading")
                .map(|el| crate::extractors::elem_text(&el))
                .unwrap_or_default()
        } else {
            title
        };

        Ok(ExtractedContent {
            content_html: html,
            title: if title.is_empty() { None } else { Some(title) },
            author: None,
            site: Some("Wikipedia".to_string()),
            published: None,
            description: None,
            schema_overrides: vec![],
        })
    }
}

fn strip_wikipedia_suffix(s: &str) -> String {
    // Strip trailing " - Wikipedia", " – Wikipedia", " — Wikipedia"
    let suffixes = [" - Wikipedia", " – Wikipedia", " — Wikipedia"];
    for sfx in suffixes {
        if let Some(stripped) = s.strip_suffix(sfx) {
            return stripped.trim().to_string();
        }
    }
    s.trim().to_string()
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract() {
        let e = WikipediaExtractor::new();
        let ctx = ExtractCtx::new(Some("https://en.wikipedia.org/wiki/Foo"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://example.com"), &[]);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_body_and_strips_clutter() {
        let html = r#"<html><head><meta property="og:title" content="Foo - Wikipedia"></head><body>
        <div id="mw-content-text"><div class="mw-parser-output">
        <p>Body</p>
        <div class="toc">TOCMARKER</div>
        <div class="navbox">NAVMARKER</div>
        </div></div></body></html>"#;
        let root = parse_html(html);
        let ctx = ExtractCtx::new(Some("https://en.wikipedia.org/wiki/Foo"), &[]);
        let e = WikipediaExtractor::new();
        let out = e.extract(&ctx, &root).unwrap();
        assert_eq!(out.title.as_deref(), Some("Foo"));
        assert_eq!(out.site.as_deref(), Some("Wikipedia"));
        assert!(out.content_html.contains("Body"));
        assert!(!out.content_html.contains("TOCMARKER"));
        assert!(!out.content_html.contains("NAVMARKER"));
    }
}
