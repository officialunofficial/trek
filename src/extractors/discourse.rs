// AGENT-P2B: Discourse extractor (host-agnostic).
//
// Port of `defuddle/src/extractors/discourse.ts`. Discourse runs on many
// self-hosted forums, so we identify it via
// `<meta name="generator" content="Discourse ...">`. Once detected, we
// treat the OP (`.topic-post.topic-owner`) as the post and remaining
// `.topic-post` siblings as replies.

use kuchikiki::NodeRef;

use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};

/// Discourse extractor.
pub struct DiscourseExtractor;

impl DiscourseExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn is_discourse(root: &NodeRef) -> bool {
        let Ok(meta) = root.select_first(r#"meta[name="generator"]"#) else {
            return false;
        };
        let attrs = meta.attributes.borrow();
        attrs
            .get("content")
            .map(|c| c.starts_with("Discourse"))
            .unwrap_or(false)
    }

    fn has_topic_post(root: &NodeRef) -> bool {
        root.select_first(".topic-post").is_ok()
    }

    fn topic_title(root: &NodeRef) -> String {
        if let Ok(t) = root.select_first(".fancy-title") {
            return t.as_node().text_contents().trim().to_string();
        }
        if let Ok(t) = root.select_first("h1[data-topic-id]") {
            return t.as_node().text_contents().trim().to_string();
        }
        String::new()
    }

    fn site_name(root: &NodeRef) -> Option<String> {
        let m = root.select_first(r#"meta[property="og:site_name"]"#).ok()?;
        let attrs = m.attributes.borrow();
        attrs.get("content").map(|s| s.to_string())
    }

    fn post_text(post: &NodeRef) -> String {
        post.select_first(".cooked")
            .ok()
            .map(|m| m.as_node().text_contents().trim().to_string())
            .unwrap_or_default()
    }

    fn post_author(post: &NodeRef) -> Option<String> {
        let link = post.select_first(".names a[data-user-card]").ok()?;
        let attrs = link.attributes.borrow();
        attrs
            .get("data-user-card")
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                drop(attrs);
                Some(link.as_node().text_contents().trim().to_string())
            })
    }
}

impl Default for DiscourseExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for DiscourseExtractor {
    fn name(&self) -> &'static str {
        "discourse"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        // URL hint: typical Discourse forums use `/t/<slug>/<id>` paths.
        // Don't make this exclusive; the DOM probe in `extract` is the
        // authoritative test.
        let _ = ctx.url;
        // Without root we'd over-claim; gate on URL hint to avoid stealing
        // arbitrary pages. Real probe in `extract`.
        ctx.url
            .map(|u| {
                regex::Regex::new(r"/t/[^/]+/\d+")
                    .map(|re| re.is_match(u))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        if !Self::is_discourse(root) {
            return Err(ExtractError::Failed {
                name: "discourse",
                reason: "no Discourse generator meta-tag".into(),
            });
        }
        if !Self::has_topic_post(root) {
            return Err(ExtractError::Failed {
                name: "discourse",
                reason: "no .topic-post in DOM".into(),
            });
        }

        let title = Self::topic_title(root);
        let site = Self::site_name(root).unwrap_or_else(|| "Discourse".to_string());

        let messages = self.extract_conversation(ctx, root)?;
        let author = messages
            .first()
            .and_then(|m| m.author.clone())
            .unwrap_or_default();

        let description = messages
            .first()
            .map(|m| {
                m.html
                    .chars()
                    .filter(|c| !matches!(c, '<' | '>'))
                    .take(140)
                    .collect::<String>()
            })
            .unwrap_or_default();

        let html = render_conversation(&messages);

        Ok(ExtractedContent {
            content_html: html,
            title: if title.is_empty() { None } else { Some(title) },
            author: if author.is_empty() {
                None
            } else {
                Some(author)
            },
            site: Some(site),
            description: Some(description),
            ..Default::default()
        })
    }
}

impl ConversationExtractor for DiscourseExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();
        let Ok(iter) = root.select(".topic-post") else {
            return Ok(messages);
        };
        for post in iter {
            let author = Self::post_author(post.as_node());
            let body = Self::post_text(post.as_node());
            // Discourse renders all replies as flat depth-0; we don't
            // attempt to derive nested depth here because Discourse's
            // tree-info lives in a JSON payload that's out of scope for
            // this pass.
            messages.push(ConversationMessage {
                author,
                timestamp: None,
                html: format!("<p>{}</p>", html_escape::encode_text(&body)),
                depth: 0,
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
    fn detects_discourse_via_generator_meta() {
        let html = r#"<html><head><meta name="generator" content="Discourse 3.2.0"></head><body><div class="topic-post topic-owner"><div class="cooked">hi</div></div></body></html>"#;
        let root = parse(html);
        assert!(DiscourseExtractor::is_discourse(&root));
        assert!(DiscourseExtractor::has_topic_post(&root));
    }

    #[test]
    fn rejects_non_discourse_html() {
        let root = parse("<html><body><p>nope</p></body></html>");
        assert!(!DiscourseExtractor::is_discourse(&root));
    }

    #[test]
    fn can_extract_uses_url_path_hint() {
        let e = DiscourseExtractor::new();
        let ctx = ExtractCtx::new(Some("https://forum.example.org/t/topic-slug/12345"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx = ExtractCtx::new(Some("https://forum.example.org/wiki/page"), &[]);
        assert!(!e.can_extract(&ctx));
    }
}
