//! ChatGPT conversation extractor.
//!
//! Matches `chatgpt.com` and `chat.openai.com` URLs and walks
//! `[data-message-author-role]` elements to collect each turn in the
//! conversation as a [`ConversationMessage`].

use kuchikiki::NodeRef;

use crate::dom::serialize;
use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};

/// Conversation extractor for ChatGPT.
pub struct ChatGptExtractor;

impl ChatGptExtractor {
    /// Construct a new extractor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ChatGptExtractor {
    fn default() -> Self {
        Self::new()
    }
}

/// Best-effort host match for ChatGPT URLs.
fn url_matches_chatgpt(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.");
    host == "chatgpt.com" || host == "chat.openai.com"
}

impl Extractor for ChatGptExtractor {
    fn name(&self) -> &'static str {
        "chatgpt"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url.is_some_and(url_matches_chatgpt)
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
            site: Some("ChatGPT".to_string()),
            ..Default::default()
        })
    }
}

impl ConversationExtractor for ChatGptExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();
        let nodes = root
            .select("[data-message-author-role]")
            .map_err(|()| ExtractError::Dom("invalid selector".to_string()))?;
        for node_data in nodes {
            let node = node_data.as_node();
            let element = match node.as_element() {
                Some(el) => el,
                None => continue,
            };
            let attrs = element.attributes.borrow();
            let role = attrs
                .get("data-message-author-role")
                .unwrap_or("")
                .to_string();
            drop(attrs);

            // Look for the message content container; fall back to the
            // message element itself if nothing more specific matches.
            let content_node =
                find_descendant_with_class(node, &["markdown", "whitespace-pre-wrap"])
                    .unwrap_or_else(|| node.clone());

            let html = serialize_inner(&content_node);
            let html = html.replace('\u{200B}', "");
            let html = html.trim().to_string();

            if html.is_empty() {
                continue;
            }

            let author = match role.as_str() {
                "user" => Some("You".to_string()),
                "assistant" => Some("ChatGPT".to_string()),
                "system" => Some("System".to_string()),
                "" => None,
                other => Some(capitalize_first(other)),
            };

            messages.push(ConversationMessage {
                author,
                timestamp: None,
                html,
                depth: 0,
            });
        }
        Ok(messages)
    }
}

/// Look for a descendant element whose `class` attribute contains any of
/// the supplied class names.
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

/// Serialize the inner HTML of a node (children only, not the node itself).
fn serialize_inner(node: &NodeRef) -> String {
    let mut out = String::new();
    for child in node.children() {
        out.push_str(&serialize(&child));
    }
    out
}

/// Capitalize the first character of a string.
fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Derive the title from the first user message's first line, capped at
/// ~80 characters with an ellipsis.
pub(crate) fn title_from_first_user_message(messages: &[ConversationMessage]) -> Option<String> {
    let first = messages.iter().find(|m| {
        m.author
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case("you") || a.eq_ignore_ascii_case("user"))
    })?;
    let text = strip_html_to_text(&first.html);
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return None;
    }
    Some(truncate_with_ellipsis(first_line, 80))
}

/// Crude HTML-to-text using kuchikiki to keep us out of regex hell.
pub(crate) fn strip_html_to_text(html: &str) -> String {
    use kuchikiki::traits::TendrilSink;
    let doc = kuchikiki::parse_html().one(html);
    doc.text_contents()
}

/// Truncate `s` to at most `max` characters, adding an ellipsis if cut.
pub(crate) fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str("...");
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
    fn can_extract_matches_chatgpt_hosts() {
        let e = ChatGptExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://chatgpt.com/c/abc-123"), &schema);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://chat.openai.com/share/xyz"), &schema);
        assert!(e.can_extract(&ctx2));
    }

    #[test]
    fn can_extract_rejects_other_hosts() {
        let e = ChatGptExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://example.com/foo"), &schema);
        assert!(!e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://claude.ai/chat/123"), &schema);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_synthetic_chatgpt_dom() {
        let html = r#"<html><body>
            <div data-testid="conversation-turn-1">
                <div data-message-author-role="user">
                    <div class="whitespace-pre-wrap">Hello, what is 2+2?</div>
                </div>
            </div>
            <div data-testid="conversation-turn-2">
                <div data-message-author-role="assistant">
                    <div class="markdown"><p>2+2 equals 4.</p></div>
                </div>
            </div>
        </body></html>"#;
        let root = parse(html);
        let e = ChatGptExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://chatgpt.com/c/test"), &schema);
        let result = e.extract(&ctx, &root).expect("extract ok");
        assert!(result.content_html.contains("conversation"));
        assert!(result.content_html.contains("2+2"));
        assert_eq!(result.site.as_deref(), Some("ChatGPT"));
        assert!(
            result.title.as_deref().unwrap_or("").contains("Hello"),
            "title should derive from first user message, got: {:?}",
            result.title
        );
        let messages = e.extract_conversation(&ctx, &root).expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].author.as_deref(), Some("You"));
        assert_eq!(messages[1].author.as_deref(), Some("ChatGPT"));
    }
}
