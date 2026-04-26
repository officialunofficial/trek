// AGENT-P2B: Mastodon extractor (host-agnostic).
//
// Port of `defuddle/src/extractors/mastodon.ts`. Mastodon runs on
// thousands of self-hosted instances, so we identify it via:
//   1. `<meta name="generator" content="Mastodon">` (most instances), OR
//   2. The presence of `#mastodon` or `script#initial-state` mentioning
//      mastodon (browser-rendered shape).
// When detected, we look for `.detailed-status__wrapper` (OP) and
// `.status__wrapper` siblings (replies / thread continuation).

use kuchikiki::NodeRef;

use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};

/// Mastodon extractor.
pub struct MastodonExtractor;

impl MastodonExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn is_mastodon(root: &NodeRef) -> bool {
        // Generator meta tag.
        if let Ok(meta) = root.select_first(r#"meta[name="generator"]"#) {
            let attrs = meta.attributes.borrow();
            if let Some(content) = attrs.get("content") {
                if content.starts_with("Mastodon") {
                    return true;
                }
            }
        }
        // `#mastodon` div (server-rendered shell).
        if root.select_first("#mastodon").is_ok() {
            return true;
        }
        // `script#initial-state` carries Mastodon JSON payload.
        if let Ok(script) = root.select_first("script#initial-state") {
            let text = script.as_node().text_contents();
            if text.contains("mastodon/mastodon") || text.contains("\"mastodon\"") {
                return true;
            }
        }
        false
    }

    fn has_main_post(root: &NodeRef) -> bool {
        root.select_first(".detailed-status__wrapper").is_ok()
    }

    fn full_handle(container: &NodeRef) -> Option<String> {
        let m = container.select_first(".display-name__account").ok()?;
        let text = m.as_node().text_contents();
        let trimmed = text.trim().trim_start_matches('@').to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    fn display_name(container: &NodeRef) -> Option<String> {
        let m = container.select_first(".display-name__html").ok()?;
        let s = m.as_node().text_contents().trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    }

    fn site_name(root: &NodeRef) -> Option<String> {
        let m = root.select_first(r#"meta[property="og:site_name"]"#).ok()?;
        let attrs = m.attributes.borrow();
        attrs.get("content").map(|s| s.to_string())
    }

    fn post_text(post: &NodeRef) -> String {
        post.select_first(".status__content__text")
            .ok()
            .map(|m| m.as_node().text_contents().trim().to_string())
            .unwrap_or_default()
    }
}

impl Default for MastodonExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for MastodonExtractor {
    fn name(&self) -> &'static str {
        "mastodon"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        // Mastodon's `can_extract` doesn't gate on URL — its DOM probe is
        // authoritative because Mastodon runs on countless domains. We
        // accept either the meta-generator tag or the detailed-status DOM
        // marker; the heavier `extract` re-validates both.
        let _ = ctx;
        // Without root we can't probe; can_extract gets root-less ctx.
        // Real probe runs in `extract`. Returning true here would short-
        // circuit later extractors, so use a URL hint as a cheap pre-filter.
        if let Some(u) = ctx.url {
            // Path pattern: `/@user/<id>`.
            if regex::Regex::new(r"/@[^/]+/\d+")
                .map(|re| re.is_match(u))
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        if !Self::is_mastodon(root) {
            return Err(ExtractError::Failed {
                name: "mastodon",
                reason: "no Mastodon generator/initial-state markers".into(),
            });
        }
        if !Self::has_main_post(root) {
            return Err(ExtractError::Failed {
                name: "mastodon",
                reason: "no detailed-status__wrapper in DOM".into(),
            });
        }

        let messages = self.extract_conversation(ctx, root)?;
        let main_handle = messages
            .first()
            .and_then(|m| m.author.clone())
            .unwrap_or_default();

        let display = root
            .select_first(".detailed-status__wrapper")
            .ok()
            .and_then(|m| Self::display_name(m.as_node()))
            .unwrap_or_default();

        let site = Self::site_name(root).unwrap_or_else(|| "Mastodon".to_string());
        // Defuddle title rule: `<author> on <site>`.
        let title_author = if display.is_empty() {
            format!("@{main_handle}")
        } else {
            display.clone()
        };
        let title = format!("{title_author} on {site}");

        let description = root
            .select_first(".detailed-status__wrapper")
            .ok()
            .map(|m| Self::post_text(m.as_node()))
            .map(|t| t.chars().take(140).collect::<String>())
            .unwrap_or_default();

        let html = render_conversation(&messages);

        Ok(ExtractedContent {
            content_html: html,
            title: Some(title),
            author: Some(if display.is_empty() {
                format!("@{main_handle}")
            } else {
                display
            }),
            site: Some(site),
            description: Some(description),
            ..Default::default()
        })
    }
}

impl ConversationExtractor for MastodonExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        let mut messages = Vec::new();

        // OP first.
        if let Ok(main) = root.select_first(".detailed-status__wrapper") {
            let handle = Self::full_handle(main.as_node()).unwrap_or_default();
            let body = Self::post_text(main.as_node());
            messages.push(ConversationMessage {
                author: if handle.is_empty() {
                    None
                } else {
                    Some(handle)
                },
                timestamp: None,
                html: format!("<p>{}</p>", html_escape::encode_text(&body)),
                depth: 0,
            });
        }

        // Then each `.status__wrapper` reply. Defuddle increments depth
        // for replies that aren't `.status--first-in-thread`.
        let Ok(iter) = root.select(".status__wrapper") else {
            return Ok(messages);
        };

        let mut depth: u32 = 0;
        for status in iter {
            // Skip the OP if it doubles up (some instances render both).
            let is_first_in_thread = status
                .as_node()
                .select_first(".status--first-in-thread")
                .is_ok();
            if is_first_in_thread {
                depth = 0;
            } else {
                depth = depth.saturating_add(1);
            }

            let handle = Self::full_handle(status.as_node()).unwrap_or_default();
            let body = Self::post_text(status.as_node());
            messages.push(ConversationMessage {
                author: if handle.is_empty() {
                    None
                } else {
                    Some(handle)
                },
                timestamp: None,
                html: format!("<p>{}</p>", html_escape::encode_text(&body)),
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
    fn detects_mastodon_via_generator_meta() {
        let html = r#"<html><head><meta name="generator" content="Mastodon 4.2.0"></head><body><div class="detailed-status__wrapper"><div class="display-name__account">@alice</div></div></body></html>"#;
        let root = parse(html);
        assert!(MastodonExtractor::is_mastodon(&root));
        assert!(MastodonExtractor::has_main_post(&root));
    }

    #[test]
    fn rejects_non_mastodon_html() {
        let root = parse("<html><body><p>nope</p></body></html>");
        assert!(!MastodonExtractor::is_mastodon(&root));
    }

    #[test]
    fn can_extract_uses_url_path_hint() {
        let e = MastodonExtractor::new();
        let ctx = ExtractCtx::new(Some("https://mastodon.social/@alice/12345"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx = ExtractCtx::new(Some("https://example.com/about"), &[]);
        assert!(!e.can_extract(&ctx));
    }
}
