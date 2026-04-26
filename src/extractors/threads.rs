// AGENT-P2B: Threads (threads.net / threads.com) extractor.
//
// Port of `defuddle/src/extractors/threads.ts`. Threads has two HTML
// shapes — pagelets (`[data-pagelet^="threads_post_page_"]`) and a
// region-based server-rendered fallback (`div[role="region"]` with
// `/@user` links). We probe both and walk `[data-pressable-container]`
// post containers to build the message list.

use kuchikiki::iter::NodeIterator;

use kuchikiki::{ElementData, NodeRef};
use once_cell::sync::Lazy;
use regex::Regex;

use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};

static THREADS_URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^https?://(?:www\.)?threads\.(?:net|com)/").expect("valid regex")
});

/// Threads extractor.
pub struct ThreadsExtractor;

impl ThreadsExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn url_match(url: Option<&str>) -> bool {
        url.is_some_and(|u| THREADS_URL.is_match(u))
    }

    fn has_pagelets(root: &NodeRef) -> bool {
        // We can't use attribute-prefix selectors directly with kuchikiki's
        // selector parser ([attr^=...] *is* supported, so use it).
        root.select_first(r#"[data-pagelet^="threads_post_page_"]"#)
            .is_ok()
    }

    fn has_region_fallback(root: &NodeRef) -> bool {
        if let Ok(region) = root.select_first(r#"div[role="region"]"#) {
            return region.as_node().select_first(r#"a[href^="/@"]"#).is_ok();
        }
        false
    }

    fn extract_username(container: &NodeRef) -> Option<String> {
        // First non-avatar `/@` link.
        let links = container.select(r#"a[href^="/@"]"#).ok()?;
        for link in links {
            let text = link.as_node().text_contents().trim().to_string();
            if !text.is_empty() && !text.contains("profile picture") {
                return Some(text);
            }
        }
        // Fallback: extract from the href.
        let first = container.select_first(r#"a[href^="/@"]"#).ok()?;
        let attrs = first.attributes.borrow();
        let href = attrs.get("href")?;
        Regex::new(r"/@([^/]+)")
            .ok()
            .and_then(|re| re.captures(href).and_then(|c| c.get(1)))
            .map(|m| m.as_str().to_string())
    }
}

impl Default for ThreadsExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for ThreadsExtractor {
    fn name(&self) -> &'static str {
        "threads"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        Self::url_match(ctx.url)
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        if !Self::has_pagelets(root) && !Self::has_region_fallback(root) {
            return Err(ExtractError::Failed {
                name: "threads",
                reason: "no Threads pagelet or region container in DOM".into(),
            });
        }

        let messages = self.extract_conversation(ctx, root)?;
        if messages.is_empty() {
            return Err(ExtractError::Failed {
                name: "threads",
                reason: "no posts found".into(),
            });
        }

        let main_username = messages
            .first()
            .and_then(|m| m.author.clone())
            .unwrap_or_default();
        let author = format!("@{main_username}");
        let title = format!("{author} on Threads");
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
            site: Some("Threads".to_string()),
            description: Some(description),
            published,
            ..Default::default()
        })
    }
}

impl ConversationExtractor for ThreadsExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();

        // Walk every pressable container as a post.
        let Ok(iter) = root.select(r#"[data-pressable-container]"#) else {
            return Ok(messages);
        };

        // Track the first author so self-replies get depth 0.
        let mut first_author: Option<String> = None;
        let mut depth: u32 = 0;

        for container in iter {
            // Skip nested pressable inside another pressable (quoted posts).
            let is_nested = container.as_node().ancestors().elements().any(
                |el: kuchikiki::NodeDataRef<ElementData>| {
                    el.attributes.borrow().contains("data-pressable-container")
                },
            );
            if is_nested {
                continue;
            }

            let username = Self::extract_username(container.as_node()).unwrap_or_default();
            if username.is_empty() {
                continue;
            }

            if first_author.is_none() {
                first_author = Some(username.clone());
                depth = 0;
            } else if Some(&username) == first_author.as_ref() {
                depth = 0;
            } else {
                depth = depth.saturating_add(1);
            }

            // Pull a `time[datetime]` if present.
            let ts = container.as_node().select_first("time").ok().and_then(|t| {
                let a = t.attributes.borrow();
                a.get("datetime").map(|s| s.to_string())
            });

            // Pull text from first `span[dir=auto]` inside the container as
            // a coarse approximation of the post body.
            let body_text = container
                .as_node()
                .select_first(r#"span[dir="auto"]"#)
                .ok()
                .map(|m| m.as_node().text_contents().trim().to_string())
                .unwrap_or_default();

            messages.push(ConversationMessage {
                author: Some(username),
                timestamp: ts,
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
    fn url_match_accepts_threads_hosts() {
        let e = ThreadsExtractor::new();
        for u in [
            "https://threads.net/@u/post/123",
            "https://www.threads.com/@u/post/456",
        ] {
            let ctx = ExtractCtx::new(Some(u), &[]);
            assert!(e.can_extract(&ctx), "should match {u}");
        }
    }

    #[test]
    fn url_match_rejects_non_threads() {
        let e = ThreadsExtractor::new();
        let ctx = ExtractCtx::new(Some("https://example.com/@u/post/1"), &[]);
        assert!(!e.can_extract(&ctx));
    }

    #[test]
    fn extract_fails_without_pagelets_or_region() {
        let e = ThreadsExtractor::new();
        let root = parse("<html><body></body></html>");
        let ctx = ExtractCtx::new(Some("https://threads.net/@u/post/1"), &[]);
        assert!(e.extract(&ctx, &root).is_err());
    }
}
