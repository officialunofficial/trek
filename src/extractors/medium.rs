//! Medium article extractor — port of Defuddle's `medium.ts`.
//!
//! Triggers on `medium.com` (and `*.medium.com`), or any page whose
//! `og:site_name` / `al:android:app_name` is `Medium`. Locates the body
//! `<article class="meteredContent">` (falling back to the first `<article>`),
//! strips Medium UI chrome, and emits the cleaned article HTML.
// AGENT-P2C: Phase 2C knowledge extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{
    elem_text, find_first, find_first_in, host_matches_suffix, meta_property, remove_all,
    serialize_node,
};

/// Medium (`medium.com` / `*.medium.com`) extractor.
pub struct MediumExtractor;

impl MediumExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for MediumExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn url_is_medium(url: &str) -> bool {
    let Ok(p) = url::Url::parse(url) else {
        return false;
    };
    let Some(h) = p.host_str() else { return false };
    h == "medium.com" || h.ends_with(".medium.com")
}

impl Extractor for MediumExtractor {
    fn name(&self) -> &'static str {
        "medium"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        // URL match always wins; otherwise check meta tags via DOM probe in extract().
        ctx.url.is_some_and(url_is_medium)
            || ctx
                .url
                .is_some_and(|u| host_matches_suffix(u, "medium.com"))
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let article =
            find_first(root, "article.meteredContent").or_else(|| find_first(root, "article"));
        let article = article.ok_or_else(|| ExtractError::Failed {
            name: "medium",
            reason: "no <article> found".into(),
        })?;

        // Verify it's actually Medium when URL didn't already say so.
        let site_name = meta_property(root, "og:site_name").unwrap_or_default();
        if site_name != "Medium" {
            // Could still be a Medium-hosted custom domain (al:android:app_name).
            let app_name = meta_property(root, "al:android:app_name").unwrap_or_default();
            let is_metered = article
                .as_element()
                .map(|el| {
                    el.attributes
                        .borrow()
                        .get("class")
                        .map(|c| c.contains("meteredContent"))
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if !is_metered && app_name != "Medium" {
                return Err(ExtractError::Failed {
                    name: "medium",
                    reason: "not a Medium page (no og:site_name=Medium)".into(),
                });
            }
        }

        clean_article(&article);

        let title = find_first_in(&article, "h1")
            .map(|el| elem_text(&el))
            .filter(|s| !s.is_empty());
        let author = find_first(root, "[data-testid=\"authorName\"]")
            .map(|el| elem_text(&el))
            .filter(|s| !s.is_empty());
        let publication = {
            let s = meta_property(root, "og:site_name").unwrap_or_default();
            if !s.is_empty() && s != "Medium" {
                Some(s)
            } else {
                None
            }
        };

        let description = find_first(root, ".pw-subtitle-paragraph")
            .map(|el| elem_text(&el))
            .filter(|s| !s.is_empty())
            .or_else(|| meta_property(root, "og:description"));

        let html = serialize_node(&article);

        Ok(ExtractedContent {
            content_html: html,
            title,
            author,
            site: Some(publication.unwrap_or_else(|| "Medium".to_string())),
            published: None,
            description,
            schema_overrides: vec![],
        })
    }
}

fn clean_article(article: &NodeRef) {
    // Remove engagement / nav controls.
    for sel in &[
        "[data-testid=\"post-preview\"]",
        "[data-testid=\"authorPhoto\"]",
        "[data-testid=\"authorName\"]",
        "[data-testid=\"storyReadTime\"]",
    ] {
        remove_all(article, sel);
    }
    // role=button → unwrap not implemented; just remove on figures since it's
    // commonly an overlay we don't need.
    remove_all(article, "figure [role=\"button\"]");
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract_url() {
        let e = MediumExtractor::new();
        let ctx = ExtractCtx::new(Some("https://medium.com/@user/some-post"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://towardsdatascience.medium.com/x"), &[]);
        assert!(e.can_extract(&ctx2));
        let ctx3 = ExtractCtx::new(Some("https://example.com"), &[]);
        assert!(!e.can_extract(&ctx3));
    }

    #[test]
    fn extracts_article_body() {
        let html = r#"<html><head><meta property="og:site_name" content="Medium"></head>
        <body><article class="meteredContent"><h1>My Story</h1><p>Body content here.</p></article></body></html>"#;
        let root = parse_html(html);
        let e = MediumExtractor::new();
        let ctx = ExtractCtx::new(Some("https://medium.com/x"), &[]);
        let out = e.extract(&ctx, &root).unwrap();
        assert_eq!(out.title.as_deref(), Some("My Story"));
        assert_eq!(out.site.as_deref(), Some("Medium"));
        assert!(out.content_html.contains("Body content here."));
    }
}
