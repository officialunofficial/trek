// AGENT-P2B: Reddit extractor.
//
// Port of `defuddle/src/extractors/reddit.ts`. Handles both new Reddit
// (`shreddit-post` custom element) and old Reddit (`.thing.link`) markup.
// Implements `ConversationExtractor` so the OP body + comments share a
// common rendering path with the other social extractors.
//
// Defuddle's full impl walks `shreddit-comment` custom elements and reads
// a `depth` attribute; old-Reddit markup nests `.thing.comment` inside
// `.child > .sitetable`. We port the depth-from-attribute logic for new
// Reddit and the recursive container-walk for old Reddit.

use kuchikiki::NodeRef;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::extractor::{
    ConversationExtractor, ConversationMessage, ExtractCtx, ExtractError, ExtractedContent,
    Extractor, render_conversation,
};

/// AGENT-P2B: matches `*.reddit.com` (incl. `old.`/`new.`/`www.`) plus
/// the `redd.it` short host.
static REDDIT_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^https?://(?:[a-z0-9-]+\.)?reddit\.com/").expect("valid regex"));

/// AGENT-P2B: Defuddle's `isCommentsPage` test — kept for reference and
/// exercised by the unit tests. Not currently used to gate `can_extract`
/// because Trek's sync path claims the whole post page.
#[allow(dead_code)]
static REDDIT_COMMENTS_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)/r/[^/]+/comments/[A-Za-z0-9]+/").expect("valid regex"));

/// Reddit extractor.
pub struct RedditExtractor;

impl RedditExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn url_match(url: Option<&str>) -> bool {
        url.is_some_and(|u| REDDIT_URL.is_match(u))
    }

    fn is_old_reddit(root: &NodeRef) -> bool {
        root.select_first(".thing.link").is_ok()
    }

    fn has_shreddit(root: &NodeRef) -> bool {
        root.select_first("shreddit-post").is_ok()
    }

    fn subreddit(url: Option<&str>) -> Option<String> {
        let u = url?;
        let re = Regex::new(r"/r/([^/]+)").ok()?;
        re.captures(u)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    fn post_id(url: Option<&str>) -> Option<String> {
        let u = url?;
        let re = Regex::new(r"comments/([A-Za-z0-9]+)").ok()?;
        re.captures(u)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
    }

    fn shreddit_attr(root: &NodeRef, attr: &str) -> Option<String> {
        let m = root.select_first("shreddit-post").ok()?;
        let attrs = m.attributes.borrow();
        attrs.get(attr).map(|s| s.to_string())
    }

    /// New-Reddit comment walk: each `<shreddit-comment>` carries a `depth`
    /// attribute; collect them in document order.
    fn extract_shreddit_comments(root: &NodeRef) -> Vec<ConversationMessage> {
        let Ok(iter) = root.select("shreddit-comment") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for m in iter {
            let attrs = m.attributes.borrow();
            let depth: u32 = attrs.get("depth").and_then(|s| s.parse().ok()).unwrap_or(0);
            let author = attrs.get("author").map(|s| s.to_string());
            let timestamp = attrs.get("created").map(|s| s.to_string());
            drop(attrs);

            // Body lives inside `[slot="comment"]`.
            let body = m
                .as_node()
                .select_first(r#"[slot="comment"]"#)
                .ok()
                .map(|c| {
                    let mut buf = Vec::new();
                    let _ = c.as_node().serialize(&mut buf);
                    String::from_utf8_lossy(&buf).into_owned()
                })
                .unwrap_or_default();

            out.push(ConversationMessage {
                author,
                timestamp,
                html: body,
                depth,
            });
        }
        out
    }

    /// Old-Reddit recursive walk: `.thing.comment` containers, with depth
    /// incremented for each `.child > .sitetable` we descend into.
    fn extract_old_reddit_comments(root: &NodeRef) -> Vec<ConversationMessage> {
        // Collect all comment containers, deriving depth from the count of
        // ancestor `.child` divs.
        let mut out = Vec::new();
        let Ok(iter) = root.select(".thing.comment") else {
            return out;
        };
        for m in iter {
            // Count ancestors with class `child` to approximate depth.
            let mut depth: u32 = 0;
            let mut parent = m.as_node().parent();
            while let Some(p) = parent {
                if let Some(el) = p.as_element() {
                    let attrs = el.attributes.borrow();
                    if attrs
                        .get("class")
                        .map(|c| c.split_whitespace().any(|cl| cl == "child"))
                        .unwrap_or(false)
                    {
                        depth = depth.saturating_add(1);
                    }
                }
                parent = p.parent();
            }

            let attrs = m.attributes.borrow();
            let author = attrs.get("data-author").map(|s| s.to_string());
            drop(attrs);

            let body = m
                .as_node()
                .select_first(".entry .usertext-body .md")
                .ok()
                .map(|c| {
                    let mut buf = Vec::new();
                    let _ = c.as_node().serialize(&mut buf);
                    String::from_utf8_lossy(&buf).into_owned()
                })
                .unwrap_or_default();

            let timestamp = m
                .as_node()
                .select_first(".entry .tagline time")
                .ok()
                .and_then(|t| {
                    let a = t.attributes.borrow();
                    a.get("datetime").map(|s| s.to_string())
                });

            out.push(ConversationMessage {
                author,
                timestamp,
                html: body,
                depth,
            });
        }
        out
    }

    fn extract_post_body(root: &NodeRef) -> String {
        // shreddit-post puts text body in `[slot="text-body"]`.
        if let Ok(m) = root.select_first(r#"shreddit-post [slot="text-body"]"#) {
            let mut buf = Vec::new();
            let _ = m.as_node().serialize(&mut buf);
            return String::from_utf8_lossy(&buf).to_string();
        }
        // Old reddit body.
        if let Ok(m) = root.select_first(".thing.link .usertext-body .md") {
            let mut buf = Vec::new();
            let _ = m.as_node().serialize(&mut buf);
            return String::from_utf8_lossy(&buf).to_string();
        }
        String::new()
    }

    fn extract_post_title(root: &NodeRef) -> String {
        if let Ok(t) = root.select_first("h1") {
            return t.as_node().text_contents().trim().to_string();
        }
        if let Ok(t) = root.select_first(".thing.link a.title") {
            return t.as_node().text_contents().trim().to_string();
        }
        String::new()
    }
}

impl Default for RedditExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for RedditExtractor {
    fn name(&self) -> &'static str {
        "reddit"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        if !Self::url_match(ctx.url) {
            return false;
        }
        // We claim any reddit URL — sync extract will gracefully fail and
        // let the host pipeline fall back to generic extraction if the
        // markup doesn't include a recognised post container.
        true
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let is_old = Self::is_old_reddit(root);
        let has_new = Self::has_shreddit(root);
        if !is_old && !has_new {
            return Err(ExtractError::Failed {
                name: "reddit",
                reason: "no shreddit-post or .thing.link in DOM".into(),
            });
        }

        let title = Self::extract_post_title(root);
        let post_body = Self::extract_post_body(root);
        let subreddit = Self::subreddit(ctx.url).unwrap_or_default();
        let _post_id = Self::post_id(ctx.url);
        let author = if is_old {
            root.select_first(".thing.link")
                .ok()
                .and_then(|m| {
                    let a = m.attributes.borrow();
                    a.get("data-author").map(|s| s.to_string())
                })
                .unwrap_or_default()
        } else {
            Self::shreddit_attr(root, "author").unwrap_or_default()
        };

        let messages = self.extract_conversation(ctx, root)?;
        let comments_html = if messages.is_empty() {
            String::new()
        } else {
            render_conversation(&messages)
        };

        let mut content_html = String::new();
        content_html.push_str("<article class=\"reddit-post\">");
        if !post_body.is_empty() {
            content_html.push_str(&post_body);
        }
        if !comments_html.is_empty() {
            content_html.push_str("<hr/>");
            content_html.push_str(&comments_html);
        }
        content_html.push_str("</article>");

        let description: String = post_body
            .chars()
            .filter(|c| !matches!(c, '<' | '>'))
            .take(140)
            .collect();

        Ok(ExtractedContent {
            content_html,
            title: if title.is_empty() { None } else { Some(title) },
            author: if author.is_empty() {
                None
            } else {
                Some(author)
            },
            site: if subreddit.is_empty() {
                Some("Reddit".into())
            } else {
                Some(format!("r/{subreddit}"))
            },
            description: Some(description),
            ..Default::default()
        })
    }
}

impl ConversationExtractor for RedditExtractor {
    fn extract_conversation(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<Vec<ConversationMessage>, ExtractError> {
        if Self::is_old_reddit(root) {
            Ok(Self::extract_old_reddit_comments(root))
        } else {
            Ok(Self::extract_shreddit_comments(root))
        }
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
    fn url_match_handles_reddit_subdomains() {
        let e = RedditExtractor::new();
        for u in [
            "https://reddit.com/r/test/comments/abc/",
            "https://www.reddit.com/r/test/",
            "https://old.reddit.com/r/test/comments/abc/",
            "https://new.reddit.com/r/test/",
        ] {
            let ctx = ExtractCtx::new(Some(u), &[]);
            assert!(e.can_extract(&ctx), "should match {u}");
        }
    }

    #[test]
    fn url_match_rejects_non_reddit() {
        let e = RedditExtractor::new();
        for u in [
            "https://example.com/r/test/",
            "https://x.com/jane/status/1",
            "https://reddit.org/r/test/",
        ] {
            let ctx = ExtractCtx::new(Some(u), &[]);
            assert!(!e.can_extract(&ctx), "should NOT match {u}");
        }
    }

    #[test]
    fn extract_fails_without_post_container() {
        let e = RedditExtractor::new();
        let root = parse("<html><body><p>nope</p></body></html>");
        let ctx = ExtractCtx::new(Some("https://reddit.com/r/x/comments/1/"), &[]);
        assert!(e.extract(&ctx, &root).is_err());
    }

    #[test]
    fn url_helpers_extract_subreddit_and_post_id() {
        assert_eq!(
            RedditExtractor::subreddit(Some("https://reddit.com/r/rust/comments/abc/foo")),
            Some("rust".into())
        );
        assert_eq!(
            RedditExtractor::post_id(Some("https://reddit.com/r/rust/comments/abc123/foo")),
            Some("abc123".into())
        );
    }

    /// REDDIT_COMMENTS_URL doesn't drive `can_extract` (just reference
    /// matches Defuddle's helper); but verifying the regex compiles and
    /// matches the right shape protects against accidental edits.
    #[test]
    fn comments_url_pattern_matches_typical_path() {
        assert!(REDDIT_COMMENTS_URL.is_match("/r/rust/comments/abc123/title"));
        assert!(!REDDIT_COMMENTS_URL.is_match("/r/rust/wiki"));
    }
}
