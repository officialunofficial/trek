// AGENT-P2B: X-Article extractor — long-form `x.com/i/article/...` /
// `twitter.com/i/article/...` / `x.com/<user>/article/<id>` posts.
//
// Port of `defuddle/src/extractors/x-article.ts`. Registered *before*
// `TwitterExtractor` so a long-form X article never gets mis-classified as
// a tweet. Marked `prefers_async = true` because the rich article body
// generally needs an FxTwitter / oEmbed fetch when no DOM is available;
// the sync path falls back to scraping any `[data-testid="twitterArticleRichTextView"]`
// container that's already in the DOM (browser-rendered case).

use kuchikiki::NodeRef;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};

/// AGENT-P2B: precise URL pattern — matches `/<user>/article/<id>` or
/// `/i/article/<id>` on x.com or twitter.com. Defuddle does the same probe.
static X_ARTICLE_URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^https?://(?:www\.|mobile\.)?(?:x|twitter)\.com/(?:[A-Za-z0-9_]{1,15}|i)/article/\d+",
    )
    .expect("valid regex")
});

/// X-Article extractor.
pub struct XArticleExtractor;

impl XArticleExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn url_match(url: Option<&str>) -> bool {
        url.is_some_and(|u| X_ARTICLE_URL.is_match(u))
    }

    /// Look for the rich-text article container Defuddle uses.
    fn has_article_container(root: &NodeRef) -> bool {
        root.select_first(r#"[data-testid="twitterArticleRichTextView"]"#)
            .is_ok()
    }

    fn article_id(url: Option<&str>) -> Option<String> {
        let u = url?;
        let re = Regex::new(r"article/(\d+)").ok()?;
        re.captures(u)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    fn author_from_url(url: Option<&str>) -> Option<String> {
        let u = url?;
        let re = Regex::new(r"/([A-Za-z0-9_]{1,15})/(?:article|status)/\d+").ok()?;
        re.captures(u)
            .and_then(|c| c.get(1))
            .map(|m| format!("@{}", m.as_str()))
    }

    /// Pull text content of the first matching element.
    fn text_of(root: &NodeRef, selector: &str) -> Option<String> {
        root.select_first(selector).ok().map(|m| {
            m.as_node()
                .text_contents()
                .trim()
                .replace(['\n', '\r'], " ")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
    }

    /// Pull the `content` attribute of the first matching element.
    fn attr_content(root: &NodeRef, selector: &str) -> Option<String> {
        let m = root.select_first(selector).ok()?;
        let attrs = m.attributes.borrow();
        attrs.get("content").map(|s| s.to_string())
    }
}

impl Default for XArticleExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for XArticleExtractor {
    fn name(&self) -> &'static str {
        "x-article"
    }

    fn prefers_async(&self) -> bool {
        // FxTwitter / oEmbed gives the richest body when an async fetcher
        // is available. Trek's sync registry currently skips
        // `prefers_async`, so leaving this at `false` (the default) lets
        // us still claim x.com article URLs in the sync path and scrape
        // any present article-element. Set to `true` once the async
        // registry is wired and an FxTwitter fetcher is plumbed through.
        false
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        if !Self::url_match(ctx.url) {
            return false;
        }
        // Without a fetcher we still want to win precedence over Twitter
        // when the rich-text container is present.
        true
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        // Try the synchronous DOM scrape first.
        if Self::has_article_container(root) {
            let title = Self::text_of(root, r#"[data-testid="twitter-article-title"]"#)
                .filter(|t| !t.is_empty())
                .unwrap_or_else(|| "Untitled X Article".to_string());

            let author_name =
                Self::attr_content(root, r#"[itemprop="author"] meta[itemprop="name"]"#);
            let author_handle = Self::attr_content(
                root,
                r#"[itemprop="author"] meta[itemprop="additionalName"]"#,
            );
            let author = match (author_name, author_handle) {
                (Some(n), Some(h)) => format!("{n} (@{h})"),
                (Some(n), None) => n,
                (None, Some(h)) => format!("@{h}"),
                (None, None) => Self::author_from_url(ctx.url).unwrap_or_else(|| "Unknown".into()),
            };

            // Serialize the article container.
            let container = root
                .select_first(r#"[data-testid="twitterArticleRichTextView"]"#)
                .map_err(|()| ExtractError::Dom("article container missing".into()))?;
            let mut body_buf: Vec<u8> = Vec::new();
            container
                .as_node()
                .serialize(&mut body_buf)
                .map_err(|e| ExtractError::Dom(e.to_string()))?;
            let body = String::from_utf8_lossy(&body_buf).to_string();
            let article_html = format!("<article class=\"x-article\">{body}</article>");

            let description = container
                .as_node()
                .text_contents()
                .trim()
                .chars()
                .take(140)
                .collect::<String>();

            return Ok(ExtractedContent {
                content_html: article_html,
                title: Some(title),
                author: Some(author),
                site: Some("X (Twitter)".to_string()),
                description: Some(description),
                ..Default::default()
            });
        }

        // Sync path with no rich-text container — fail softly so the
        // host pipeline falls back to generic extraction. With a fetcher
        // (async path), an extended impl would call FxTwitter here.
        let _ = Self::article_id(ctx.url);
        Err(ExtractError::Failed {
            name: "x-article",
            reason: "no twitterArticleRichTextView in DOM and no async fetcher available"
                .to_string(),
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
    fn matches_x_com_article_url() {
        let e = XArticleExtractor::new();
        let root = parse("<html><body></body></html>");
        let ctx = ExtractCtx::new(Some("https://x.com/i/article/1234567890"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://twitter.com/jane/article/9999"), &[]);
        assert!(e.can_extract(&ctx2));
        let _ = root;
    }

    #[test]
    fn rejects_non_article_x_urls() {
        let e = XArticleExtractor::new();
        // Plain status URL must NOT match.
        let ctx = ExtractCtx::new(Some("https://x.com/jane/status/12345"), &[]);
        assert!(!e.can_extract(&ctx));
        // Unrelated host.
        let ctx = ExtractCtx::new(Some("https://example.com/i/article/1"), &[]);
        assert!(!e.can_extract(&ctx));
    }

    #[test]
    fn name_is_stable() {
        assert_eq!(XArticleExtractor::new().name(), "x-article");
        // prefers_async stays false in this phase — see impl comment.
        assert!(!XArticleExtractor::new().prefers_async());
    }
}
