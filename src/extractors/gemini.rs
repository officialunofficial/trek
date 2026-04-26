//! Gemini conversation extractor.
//!
//! Matches `gemini.google.com` and `bard.google.com`. Walks
//! `div.conversation-container` blocks and pulls the user query out of
//! `user-query .query-text` and the model's reply out of
//! `model-response .markdown` (or the extended-response markdown when
//! present).

use kuchikiki::NodeRef;

use crate::dom::serialize;
use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};
use crate::extractors::chatgpt::title_from_first_user_message;

/// Conversation extractor for Gemini.
pub struct GeminiExtractor;

impl GeminiExtractor {
    /// Construct a new extractor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for GeminiExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn url_matches_gemini(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.");
    host == "gemini.google.com" || host == "bard.google.com"
}

impl Extractor for GeminiExtractor {
    fn name(&self) -> &'static str {
        "gemini"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url.is_some_and(url_matches_gemini)
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
            site: Some("Gemini".to_string()),
            ..Default::default()
        })
    }
}

impl ConversationExtractor for GeminiExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();
        let containers = root
            .select("div.conversation-container")
            .map_err(|()| ExtractError::Dom("invalid selector".to_string()))?;

        for container_data in containers {
            let container = container_data.as_node();

            // User query: <user-query>...<.query-text>...
            if let Some(user_query) = find_descendant_by_tag(container, "user-query") {
                if let Some(query_text) = find_descendant_with_class(&user_query, &["query-text"]) {
                    let html = serialize_inner(&query_text).trim().to_string();
                    if !html.is_empty() {
                        messages.push(ConversationMessage {
                            author: Some("You".to_string()),
                            timestamp: None,
                            html,
                            depth: 0,
                        });
                    }
                }
            }

            // Model response: <model-response> with .markdown content;
            // prefer the extended-response variant when present.
            if let Some(model_response) = find_descendant_by_tag(container, "model-response") {
                let extended =
                    find_descendant_with_id(&model_response, "extended-response-markdown-content");
                let regular_markdown = find_descendant_with_class(&model_response, &["markdown"]);
                let content = extended.or(regular_markdown);
                if let Some(node) = content {
                    let html = serialize_inner(&node).trim().to_string();
                    if !html.is_empty() {
                        messages.push(ConversationMessage {
                            author: Some("Gemini".to_string()),
                            timestamp: None,
                            html,
                            depth: 0,
                        });
                    }
                }
            }
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

fn find_descendant_with_id(node: &NodeRef, id: &str) -> Option<NodeRef> {
    for desc in node.descendants() {
        if let Some(el) = desc.as_element() {
            let attrs = el.attributes.borrow();
            if attrs.get("id") == Some(id) {
                return Some(desc.clone());
            }
        }
    }
    None
}

fn find_descendant_by_tag(node: &NodeRef, tag: &str) -> Option<NodeRef> {
    for desc in node.descendants() {
        if let Some(el) = desc.as_element() {
            if &*el.name.local == tag {
                return Some(desc.clone());
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
    fn can_extract_matches_gemini_hosts() {
        let e = GeminiExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://gemini.google.com/app/abc"), &schema);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://bard.google.com/share/xyz"), &schema);
        assert!(e.can_extract(&ctx2));
    }

    #[test]
    fn can_extract_rejects_other_hosts() {
        let e = GeminiExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://google.com/search?q=foo"), &schema);
        assert!(!e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://chatgpt.com/c/foo"), &schema);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_synthetic_gemini_dom() {
        let html = r#"<html><body>
            <div class="conversation-container">
                <user-query>
                    <div class="query-text">Tell me a joke about cats.</div>
                </user-query>
                <model-response>
                    <div class="model-response-text">
                        <div class="markdown"><p>Why don't cats play poker in the jungle? Too many cheetahs.</p></div>
                    </div>
                </model-response>
            </div>
        </body></html>"#;
        let root = parse(html);
        let e = GeminiExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://gemini.google.com/app/test"), &schema);
        let messages = e.extract_conversation(&ctx, &root).expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].author.as_deref(), Some("You"));
        assert!(messages[0].html.contains("joke about cats"));
        assert_eq!(messages[1].author.as_deref(), Some("Gemini"));
        assert!(messages[1].html.contains("cheetahs"));

        let result = e.extract(&ctx, &root).expect("extract ok");
        assert_eq!(result.site.as_deref(), Some("Gemini"));
        assert!(result.title.as_deref().unwrap_or("").contains("joke"));
    }
}
