//! LeetCode extractor — port of Defuddle's `leetcode.ts`.
//!
//! Triggers on `leetcode.com/problems/...`. Best-effort SSR extraction:
//! reads `[data-track-load="description_content"]` if present and strips
//! the premium-upsell containers. Title is the og:title minus the
//! trailing `- LeetCode` suffix.
// AGENT-P2C: Phase 2C dev extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{
    find_first, host_matches_exact, meta_property, remove_all, serialize_node,
};

/// LeetCode (`leetcode.com`) extractor.
pub struct LeetCodeExtractor;

impl LeetCodeExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for LeetCodeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn strip_leetcode_suffix(s: &str) -> String {
    for sfx in [" - LeetCode", " – LeetCode", " — LeetCode"] {
        if let Some(stripped) = s.strip_suffix(sfx) {
            return stripped.trim().to_string();
        }
    }
    s.trim().to_string()
}

impl Extractor for LeetCodeExtractor {
    fn name(&self) -> &'static str {
        "leetcode"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url
            .is_some_and(|u| host_matches_exact(u, "leetcode.com"))
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let body = find_first(root, "[data-track-load=\"description_content\"]");
        let body = body.ok_or_else(|| ExtractError::Failed {
            name: "leetcode",
            reason: "no [data-track-load=description_content]".into(),
        })?;

        // Strip premium upsell.
        for sel in &[
            ".premium-content",
            "[data-track-load=\"premium_paywall\"]",
            ".lock-icon",
            "[class*=\"premium\"]",
        ] {
            remove_all(&body, sel);
        }

        let html = serialize_node(&body);
        let og_title = meta_property(root, "og:title").unwrap_or_default();
        let title = strip_leetcode_suffix(&og_title);

        Ok(ExtractedContent {
            content_html: html,
            title: if title.is_empty() { None } else { Some(title) },
            author: None,
            site: Some("LeetCode".to_string()),
            published: None,
            description: None,
            schema_overrides: vec![],
        })
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract_url() {
        let e = LeetCodeExtractor::new();
        let ctx = ExtractCtx::new(Some("https://leetcode.com/problems/two-sum/"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://example.com"), &[]);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_description_content() {
        let html = r#"<html><head><meta property="og:title" content="Two Sum - LeetCode"></head>
        <body><div data-track-load="description_content"><p>Given an array...</p></div></body></html>"#;
        let root = parse_html(html);
        let e = LeetCodeExtractor::new();
        let ctx = ExtractCtx::new(Some("https://leetcode.com/problems/two-sum"), &[]);
        let out = e.extract(&ctx, &root).unwrap();
        assert_eq!(out.title.as_deref(), Some("Two Sum"));
        assert_eq!(out.site.as_deref(), Some("LeetCode"));
        assert!(out.content_html.contains("Given an array"));
    }
}
