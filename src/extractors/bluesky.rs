// AGENT-P2B: Bluesky (bsky.app) extractor.
//
// Port of `defuddle/src/extractors/bluesky.ts`. Bluesky's web client uses
// a `[data-testid="postThreadScreen"]` outer container wrapping
// `[data-testid^="postThreadItem-by-<handle>"]` items. The first item is
// the OP; subsequent items are either further posts by the same author
// (depth 0) or replies whose connector-line styling encodes their
// nesting depth.

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};

static BSKY_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^https?://(?:www\.)?bsky\.app/").expect("valid regex"));

/// Bluesky extractor.
pub struct BlueskyExtractor;

impl BlueskyExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn url_match(url: Option<&str>) -> bool {
        url.is_some_and(|u| BSKY_URL.is_match(u))
    }

    fn has_thread_screen(root: &NodeRef) -> bool {
        root.select_first(r#"[data-testid="postThreadScreen"]"#)
            .is_ok()
    }

    fn handle_from_testid(node: &NodeRef) -> Option<String> {
        let attrs = node.as_element()?.attributes.borrow();
        let testid = attrs.get("data-testid")?;
        testid
            .strip_prefix("postThreadItem-by-")
            .map(str::to_string)
    }

    /// Detect the connector-line that means "reply to the post above".
    /// Mirrors the heuristic in `defuddle/src/extractors/bluesky.ts`.
    fn has_top_connector(item: &NodeRef) -> bool {
        let Some(first_child) = item.first_child() else {
            return false;
        };
        let Ok(divs) = first_child.select("div") else {
            return false;
        };
        for d in divs {
            let attrs = d.attributes.borrow();
            if let Some(style) = attrs.get("style") {
                if style.contains("width: 2px") && style.contains("background-color") {
                    return true;
                }
            }
        }
        false
    }
}

impl Default for BlueskyExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for BlueskyExtractor {
    fn name(&self) -> &'static str {
        "bluesky"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        Self::url_match(ctx.url)
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        if !Self::has_thread_screen(root) {
            return Err(ExtractError::Failed {
                name: "bluesky",
                reason: "no postThreadScreen container in DOM".into(),
            });
        }

        let messages = self.extract_conversation(ctx, root)?;
        if messages.is_empty() {
            return Err(ExtractError::Failed {
                name: "bluesky",
                reason: "no posts found".into(),
            });
        }

        let main_handle = messages
            .first()
            .and_then(|m| m.author.clone())
            .unwrap_or_default();
        let author = format!("@{main_handle}");
        let title = format!("{author} on Bluesky");
        let description = messages
            .first()
            .map(|m| m.html.chars().take(140).collect::<String>())
            .unwrap_or_default();
        let published = messages.first().and_then(|m| m.timestamp.clone());

        let html = render_conversation(&messages);

        Ok(ExtractedContent {
            content_html: html,
            title: Some(title),
            author: Some(author),
            site: Some("Bluesky".to_string()),
            description: Some(description),
            published,
            ..Default::default()
        })
    }
}

impl ConversationExtractor for BlueskyExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();
        let Ok(iter) = root.select(r#"[data-testid^="postThreadItem-by-"]"#) else {
            return Ok(messages);
        };

        let mut first_author: Option<String> = None;
        let mut depth: u32 = 0;
        for item in iter {
            let handle = Self::handle_from_testid(item.as_node()).unwrap_or_default();
            if handle.is_empty() {
                continue;
            }

            // Author classification + connector-line nesting.
            if first_author.is_none() {
                first_author = Some(handle.clone());
                depth = 0;
            } else if Some(&handle) == first_author.as_ref() {
                depth = 0;
            } else if Self::has_top_connector(item.as_node()) {
                depth = depth.saturating_add(1);
            } else {
                depth = 0;
            }

            // Body lives in `div[data-word-wrap="1"]`.
            let body_text = item
                .as_node()
                .select_first(r#"div[data-word-wrap="1"]"#)
                .ok()
                .map(|m| m.as_node().text_contents().trim().to_string())
                .unwrap_or_default();

            messages.push(ConversationMessage {
                author: Some(handle),
                timestamp: None,
                html: format!("<p>{}</p>", html_escape::encode_text(&body_text)),
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
    fn url_match_accepts_bsky_app() {
        let e = BlueskyExtractor::new();
        let ctx = ExtractCtx::new(Some("https://bsky.app/profile/foo/post/1"), &[]);
        assert!(e.can_extract(&ctx));
    }

    #[test]
    fn url_match_rejects_non_bsky() {
        let e = BlueskyExtractor::new();
        let ctx = ExtractCtx::new(Some("https://example.com/profile/foo/post/1"), &[]);
        assert!(!e.can_extract(&ctx));
    }

    #[test]
    fn extract_fails_without_thread_screen() {
        let e = BlueskyExtractor::new();
        let root = parse("<html><body><p>no thread</p></body></html>");
        let ctx = ExtractCtx::new(Some("https://bsky.app/x/post/1"), &[]);
        assert!(e.extract(&ctx, &root).is_err());
    }
}
