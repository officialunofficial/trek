//! Claude conversation extractor.
//!
//! Matches `claude.ai` URLs. Reads `[data-testid=user-message]` for user
//! turns and `.font-claude-response` (preferring its `.standard-markdown`
//! body when present) for assistant turns.

use kuchikiki::NodeRef;

use crate::dom::serialize;
use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};
use crate::extractors::chatgpt::title_from_first_user_message;

/// Conversation extractor for Claude.ai.
pub struct ClaudeExtractor;

impl ClaudeExtractor {
    /// Construct a new extractor.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for ClaudeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn url_matches_claude(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.");
    host == "claude.ai"
}

impl Extractor for ClaudeExtractor {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url.is_some_and(url_matches_claude)
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
            site: Some("Claude".to_string()),
            ..Default::default()
        })
    }
}

impl ConversationExtractor for ClaudeExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();
        // Defuddle's selector: user messages, assistant messages,
        // and the .font-claude-response container that wraps either.
        let nodes = root
            .select(
                "div[data-testid=\"user-message\"], \
                 div[data-testid=\"assistant-message\"], \
                 div.font-claude-response",
            )
            .map_err(|()| ExtractError::Dom("invalid selector".to_string()))?;

        for node_data in nodes {
            let node = node_data.as_node();
            let element = match node.as_element() {
                Some(el) => el,
                None => continue,
            };

            let attrs = element.attributes.borrow();
            let testid = attrs.get("data-testid").map(str::to_string);
            let class_attr = attrs.get("class").unwrap_or("").to_string();
            drop(attrs);

            let (author, content_node): (&str, NodeRef) = if let Some(tid) = testid.as_deref() {
                match tid {
                    "user-message" => ("You", node.clone()),
                    "assistant-message" => {
                        let body = find_descendant_with_class(node, &["standard-markdown"])
                            .unwrap_or_else(|| node.clone());
                        ("Claude", body)
                    }
                    _ => continue,
                }
            } else if class_attr
                .split_whitespace()
                .any(|c| c == "font-claude-response")
            {
                let body = find_descendant_with_class(node, &["standard-markdown"])
                    .unwrap_or_else(|| node.clone());
                ("Claude", body)
            } else {
                continue;
            };

            let html = serialize_inner(&content_node);
            let html = html.replace('\u{200B}', "");
            let html = html.trim().to_string();
            if html.is_empty() {
                continue;
            }

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
    fn can_extract_matches_claude_ai() {
        let e = ClaudeExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://claude.ai/chat/abc-123"), &schema);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://claude.ai/share/xyz"), &schema);
        assert!(e.can_extract(&ctx2));
    }

    #[test]
    fn can_extract_rejects_other_hosts() {
        let e = ClaudeExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://chatgpt.com/c/foo"), &schema);
        assert!(!e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://anthropic.com"), &schema);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_synthetic_claude_dom() {
        let html = r#"<html><body>
            <div data-testid="user-message"><p>What is the capital of France?</p></div>
            <div class="font-claude-response">
                <div class="standard-markdown"><p>The capital of France is Paris.</p></div>
            </div>
        </body></html>"#;
        let root = parse(html);
        let e = ClaudeExtractor::new();
        let schema = vec![];
        let ctx = ExtractCtx::new(Some("https://claude.ai/chat/test"), &schema);
        let messages = e.extract_conversation(&ctx, &root).expect("messages");
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].author.as_deref(), Some("You"));
        assert!(messages[0].html.contains("capital of France"));
        assert_eq!(messages[1].author.as_deref(), Some("Claude"));
        assert!(messages[1].html.contains("Paris"));

        let result = e.extract(&ctx, &root).expect("extract ok");
        assert_eq!(result.site.as_deref(), Some("Claude"));
        assert!(result.title.as_deref().unwrap_or("").contains("capital"));
    }
}
