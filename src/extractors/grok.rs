//! Grok conversation extractor.
//!
//! Matches `grok.com` and `x.com/i/grok`. The Grok DOM relies heavily on
//! utility classes (`items-end` for the user side, `items-start` for the
//! assistant side); both selectors below are documented in Defuddle's
//! `grok.ts` and may break as Grok's UI evolves.

use kuchikiki::NodeRef;

use crate::dom::serialize;
use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};
use crate::extractors::chatgpt::title_from_first_user_message;

/// Conversation extractor for Grok.
pub struct GrokExtractor;

impl GrokExtractor {
    /// Construct a new extractor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for GrokExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn url_matches_grok(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.");
    if host == "grok.com" {
        return true;
    }
    // x.com/i/grok or twitter.com/i/grok
    if host == "x.com" || host == "twitter.com" {
        return parsed.path().starts_with("/i/grok");
    }
    false
}

impl Extractor for GrokExtractor {
    fn name(&self) -> &'static str {
        "grok"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url.is_some_and(url_matches_grok)
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let messages = self.extract_conversation(ctx, root)?;
        let title = title_from_first_user_message(&messages);
        let content_html = render_conversation(&messages);
        Ok(ExtractedContent {
            content_html,
            title,
            site: Some("Grok".to_string()),
            ..Default::default()
        })
    }
}

impl ConversationExtractor for GrokExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();
        // Grok's container — utility-class soup. Include both the long
        // form Defuddle observed and a more lenient `.message-bubble`
        // ancestor probe so synthetic test fixtures stay readable.
        let containers = root
            .select(".relative.group.flex.flex-col.justify-center.w-full")
            .map_err(|()| ExtractError::Dom("invalid selector".to_string()))?;

        for node_data in containers {
            let node = node_data.as_node();
            let element = match node.as_element() {
                Some(el) => el,
                None => continue,
            };
            let attrs = element.attributes.borrow();
            let class_attr = attrs.get("class").unwrap_or("").to_string();
            drop(attrs);
            let class_list: Vec<&str> = class_attr.split_whitespace().collect();
            let is_user = class_list.contains(&"items-end");
            let is_grok = class_list.contains(&"items-start");
            if !is_user && !is_grok {
                continue;
            }

            let bubble = match find_descendant_with_class(node, &["message-bubble"]) {
                Some(b) => b,
                None => continue,
            };

            let (author, html) = if is_user {
                let text = bubble.text_contents().trim().to_string();
                if text.is_empty() {
                    continue;
                }
                ("You", html_escape::encode_text(&text).into_owned())
            } else {
                let inner = serialize_inner(&bubble);
                let inner = inner.trim().to_string();
                if inner.is_empty() {
                    continue;
                }
                ("Grok", inner)
            };

            messages.push(ConversationMessage {
                author: Some(author.to_string()),
                timestamp: None,
                html,
                depth: 0,
            });
        }
        Ok(messages)
    }
}

fn find_descendant_with_class(node: &NodeRef, classes: &[&str]) -> Option<NodeRef> {
    for desc in node.descendants() {
        if let Some(el) = desc.as_element() {
            let attrs = el.attributes.borrow();
            if let Some(class_attr) = attrs.get("class") {
                let class_list: Vec<&str> = class_attr.split_whitespace().collect();
                if classes.iter().any(|c| class_list.contains(c)) {
                    return Some(desc.clone());
                }
            }
        }
    }
    None
}

fn serialize_inner(node: &NodeRef) -> String {
    let mut out = String::new();
    for child in node.children() {
        out.push_str(&serialize(&child));
    }
    out
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use kuchikiki::traits::TendrilSink;

    fn parse(html: &str) -> NodeRef {
        kuchikiki::parse_html().one(html)
    }

    #[test]
    fn can_extract_matches_grok_hosts() {
        let e = GrokExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://grok.com/chat/abc"), &schema);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://x.com/i/grok"), &schema);
        assert!(e.can_extract(&ctx2));
        let ctx3 = ExtractCtx::new(Some("https://x.com/i/grok/share/xyz"), &schema);
        assert!(e.can_extract(&ctx3));
    }

    #[test]
    fn can_extract_rejects_other_hosts() {
        let e = GrokExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://x.com/jack/status/123"), &schema);
        assert!(!e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://example.com"), &schema);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_synthetic_grok_dom() {
        let html = r#"<html><body>
            <div class="relative group flex flex-col justify-center w-full items-end">
                <div class="message-bubble">Why is the sky blue?</div>
            </div>
            <div class="relative group flex flex-col justify-center w-full items-start">
                <div class="message-bubble"><p>Rayleigh scattering.</p></div>
            </div>
        </body></html>"#;
        let root = parse(html);
        let e = GrokExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://grok.com/chat/test"), &schema);
        let messages = e.extract_conversation(&ctx, &root).expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].author.as_deref(), Some("You"));
        assert!(messages[0].html.contains("sky blue"));
        assert_eq!(messages[1].author.as_deref(), Some("Grok"));
        assert!(messages[1].html.contains("Rayleigh"));

        let result = e.extract(&ctx, &root).expect("extract ok");
        assert_eq!(result.site.as_deref(), Some("Grok"));
        assert!(result.title.is_some());
    }
}
