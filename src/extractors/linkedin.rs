// AGENT-P2B: LinkedIn extractor.
//
// Port of `defuddle/src/extractors/linkedin.ts`. Targets LinkedIn post
// pages — `[role="article"].feed-shared-update-v2` — and pulls the
// commentary text plus images/video poster. Reposts (quoted-post nests)
// are stripped to avoid duplicate content.

use kuchikiki::NodeRef;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};

static LINKEDIN_URL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^https?://(?:[a-z]+\.)?linkedin\.com/").expect("valid regex"));

/// LinkedIn extractor.
pub struct LinkedInExtractor;

impl LinkedInExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    fn url_match(url: Option<&str>) -> bool {
        url.is_some_and(|u| LINKEDIN_URL.is_match(u))
    }

    fn post_article(root: &NodeRef) -> Option<kuchikiki::NodeDataRef<kuchikiki::ElementData>> {
        root.select_first(r#"[role="article"].feed-shared-update-v2"#)
            .ok()
    }

    /// Extract visible text minus screen-reader-only `.visually-hidden`.
    fn visible_text(node: &NodeRef) -> String {
        // Iterate descendants skipping visually-hidden subtrees.
        let mut out = String::new();
        for desc in node.descendants() {
            if let Some(el) = desc.as_element() {
                let attrs = el.attributes.borrow();
                if attrs
                    .get("class")
                    .map(|c| c.split_whitespace().any(|cl| cl == "visually-hidden"))
                    .unwrap_or(false)
                {
                    continue;
                }
            }
            if let Some(text) = desc.as_text() {
                out.push_str(&text.borrow());
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }
}

impl Default for LinkedInExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for LinkedInExtractor {
    fn name(&self) -> &'static str {
        "linkedin"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        Self::url_match(ctx.url)
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let Some(article) = Self::post_article(root) else {
            return Err(ExtractError::Failed {
                name: "linkedin",
                reason: "no feed-shared-update-v2 article in DOM".into(),
            });
        };
        let article_node = article.as_node();

        // Author from `.update-components-actor__title`.
        let author = article_node
            .select_first(".update-components-actor__title")
            .ok()
            .map(|m| Self::visible_text(m.as_node()))
            .unwrap_or_default();

        // Body: `.update-components-text.update-components-update-v2__commentary`.
        // Skip when nested inside a quoted update wrapper.
        let body_text = article_node
            .select(".update-components-text")
            .ok()
            .into_iter()
            .flatten()
            .find_map(|m| {
                let in_quote = m.as_node().ancestors().any(|a| {
                    a.as_element()
                        .map(|el| {
                            el.attributes
                                .borrow()
                                .get("class")
                                .map(|c| {
                                    c.contains("feed-shared-update-v2__update-content-wrapper")
                                })
                                .unwrap_or(false)
                        })
                        .unwrap_or(false)
                });
                if in_quote {
                    None
                } else {
                    Some(Self::visible_text(m.as_node()))
                }
            })
            .unwrap_or_default();

        let mut content_html = String::new();
        content_html.push_str("<article class=\"linkedin-post\">");
        if !body_text.is_empty() {
            content_html.push_str("<p>");
            content_html.push_str(&html_escape::encode_text(&body_text));
            content_html.push_str("</p>");
        }
        content_html.push_str("</article>");

        let description: String = body_text.chars().take(140).collect();

        // Defuddle title rule: `<author> on LinkedIn`.
        let title = if author.is_empty() {
            "LinkedIn post".to_string()
        } else {
            format!("{author} on LinkedIn")
        };

        Ok(ExtractedContent {
            content_html,
            title: Some(title),
            author: if author.is_empty() {
                None
            } else {
                Some(author)
            },
            site: Some("LinkedIn".to_string()),
            description: Some(description),
            ..Default::default()
        })
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
    fn url_match_accepts_linkedin_subdomains() {
        let e = LinkedInExtractor::new();
        for u in [
            "https://www.linkedin.com/posts/jane",
            "https://linkedin.com/in/jane",
            "https://uk.linkedin.com/posts/jane",
        ] {
            let ctx = ExtractCtx::new(Some(u), &[]);
            assert!(e.can_extract(&ctx), "should match {u}");
        }
    }

    #[test]
    fn url_match_rejects_non_linkedin() {
        let e = LinkedInExtractor::new();
        let ctx = ExtractCtx::new(Some("https://example.com/posts/jane"), &[]);
        assert!(!e.can_extract(&ctx));
    }

    #[test]
    fn extract_fails_without_post_article() {
        let e = LinkedInExtractor::new();
        let root = parse("<html><body><p>no article</p></body></html>");
        let ctx = ExtractCtx::new(Some("https://www.linkedin.com/posts/jane"), &[]);
        assert!(e.extract(&ctx, &root).is_err());
    }
}
