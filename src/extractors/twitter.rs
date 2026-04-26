// AGENT-P2B: Twitter / X status extractor.
//
// Port of `defuddle/src/extractors/twitter.ts`. Matches `twitter.com` and
// `x.com` status URLs *only*; Article URLs are claimed by `XArticleExtractor`
// which is registered before this one. Implements `ConversationExtractor`
// — the main tweet plus replies are walked into a flat message list and
// the registry's shared `render_conversation` helper produces the final
// HTML.

use kuchikiki::iter::NodeIterator;

use kuchikiki::{ElementData, NodeRef};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};

/// AGENT-P2B: status URL pattern excluding `/i/article/` (claimed by
/// `XArticleExtractor`).
static TWITTER_STATUS_URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^https?://(?:www\.|mobile\.)?(?:x|twitter)\.com/[A-Za-z0-9_]{1,15}/status/\d+")
        .expect("valid regex")
});

/// AGENT-P2B: also matches the Article URL form so we can explicitly skip it.
static TWITTER_ARTICLE_URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)^https?://(?:www\.|mobile\.)?(?:x|twitter)\.com/(?:[A-Za-z0-9_]{1,15}|i)/article/\d+",
    )
    .expect("valid regex")
});

/// Twitter / X status extractor.
pub struct TwitterExtractor;

impl TwitterExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn url_match(url: Option<&str>) -> bool {
        let Some(u) = url else { return false };
        if TWITTER_ARTICLE_URL.is_match(u) {
            return false;
        }
        TWITTER_STATUS_URL.is_match(u)
    }

    fn handle_from_url(url: Option<&str>) -> Option<String> {
        let u = url?;
        let re = Regex::new(r"/([A-Za-z0-9_]{1,15})/status/\d+").ok()?;
        re.captures(u)
            .and_then(|c| c.get(1))
            .map(|m| format!("@{}", m.as_str()))
    }

    fn tweet_id(url: Option<&str>) -> Option<String> {
        let u = url?;
        let re = Regex::new(r"status/(\d+)").ok()?;
        re.captures(u)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    /// Cheap DOM probe — the rendered tweet article element.
    /// AGENT-P2B: exposed for tests and future async-path probes.
    #[allow(dead_code)]
    fn has_tweet_article(root: &NodeRef) -> bool {
        root.select_first(r#"article[data-testid="tweet"]"#).is_ok()
    }

    fn tweet_text(article: &NodeRef) -> String {
        article
            .select_first(r#"[data-testid="tweetText"]"#)
            .ok()
            .map(|m| {
                m.as_node()
                    .text_contents()
                    .trim()
                    .replace(['\n', '\r'], " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default()
    }

    fn tweet_handle(article: &NodeRef) -> String {
        // The user-name block contains two anchor tags: [0] display name,
        // [1] handle (`@user`).
        let Ok(user_block) = article.select_first(r#"[data-testid="User-Name"]"#) else {
            return String::new();
        };
        let anchors: Vec<_> = user_block
            .as_node()
            .descendants()
            .elements()
            .filter(|el: &kuchikiki::NodeDataRef<ElementData>| &*el.name.local == "a")
            .collect();
        anchors
            .get(1)
            .map(|a| a.as_node().text_contents().trim().to_string())
            .unwrap_or_default()
    }

    fn tweet_datetime(article: &NodeRef) -> Option<String> {
        let time = article.select_first("time").ok()?;
        let attrs = time.attributes.borrow();
        attrs
            .get("datetime")
            .map(|s| s.split('T').next().unwrap_or(s).to_string())
    }
}

impl Default for TwitterExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for TwitterExtractor {
    fn name(&self) -> &'static str {
        "twitter"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        Self::url_match(ctx.url)
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let messages = self.extract_conversation(ctx, root)?;

        let main_handle = messages
            .first()
            .and_then(|m| m.author.clone())
            .or_else(|| Self::handle_from_url(ctx.url))
            .unwrap_or_else(|| "Unknown".to_string());

        // Defuddle's title rule for tweets: `<author> on <site>`
        let title = format!("{main_handle} on X");

        let description = messages
            .first()
            .map(|m| m.html.chars().take(140).collect::<String>())
            .unwrap_or_default();

        let published = messages.first().and_then(|m| m.timestamp.clone());

        let _ = Self::tweet_id(ctx.url);
        let html = if messages.is_empty() {
            String::new()
        } else {
            render_conversation(&messages)
        };

        if html.is_empty() {
            return Err(ExtractError::Failed {
                name: "twitter",
                reason: "no tweet article in DOM".into(),
            });
        }

        Ok(ExtractedContent {
            content_html: html,
            title: Some(title),
            author: Some(main_handle),
            site: Some("X (Twitter)".to_string()),
            description: Some(description),
            published,
            ..Default::default()
        })
    }
}

impl ConversationExtractor for TwitterExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();

        // Walk every tweet article. Defuddle uses cellInnerDiv ordering to
        // classify thread vs reply; we keep the first-author-thread heuristic
        // by tracking the first article's handle.
        let articles = root
            .select(r#"article[data-testid="tweet"]"#)
            .map_err(|()| ExtractError::Dom("tweet selector failed".into()))?;

        let mut first_handle: Option<String> = None;
        let mut depth: u32 = 0;

        for article in articles {
            let node = article.as_node();
            let handle = Self::tweet_handle(node);
            let text = Self::tweet_text(node);
            let ts = Self::tweet_datetime(node);

            if first_handle.is_none() {
                first_handle = Some(handle.clone());
                depth = 0;
            } else if Some(&handle) == first_handle.as_ref() {
                // Self-reply / thread continuation — keep depth at 0.
                depth = 0;
            } else {
                depth = depth.saturating_add(1);
            }

            messages.push(ConversationMessage {
                author: if handle.is_empty() {
                    None
                } else {
                    Some(handle)
                },
                timestamp: ts,
                html: format!("<p>{}</p>", html_escape::encode_text(&text)),
                depth,
            });
        }

        Ok(messages)
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
    fn matches_status_urls_on_x_and_twitter() {
        let e = TwitterExtractor::new();
        for u in [
            "https://x.com/jane/status/123",
            "https://twitter.com/jane/status/123",
            "https://www.x.com/jane/status/123",
            "https://mobile.twitter.com/jane/status/123",
        ] {
            let ctx = ExtractCtx::new(Some(u), &[]);
            assert!(e.can_extract(&ctx), "should match {u}");
        }
    }

    #[test]
    fn skips_x_article_urls() {
        let e = TwitterExtractor::new();
        for u in [
            "https://x.com/i/article/12345",
            "https://x.com/jane/article/12345",
            "https://twitter.com/i/article/999",
            "https://example.com/jane/status/1",
        ] {
            let ctx = ExtractCtx::new(Some(u), &[]);
            assert!(!e.can_extract(&ctx), "should NOT match {u}");
        }
    }

    #[test]
    fn name_and_basic_extract_failure() {
        let e = TwitterExtractor::new();
        assert_eq!(e.name(), "twitter");
        let root = parse("<html><body><p>nothing</p></body></html>");
        let ctx = ExtractCtx::new(Some("https://x.com/jane/status/1"), &[]);
        let _ = TwitterExtractor::has_tweet_article(&root);
        assert!(e.extract(&ctx, &root).is_err());
    }
}
