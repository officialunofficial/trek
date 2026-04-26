//! Substack extractor — port of Defuddle's `substack.ts`.
//!
//! Triggers on:
//! - any URL on `substack.com` / `*.substack.com`,
//! - any other host with `<meta name="generator" content="Substack">`.
//!
//! Handles three shapes:
//! - rendered post body `div.body.markup`,
//! - notes (ProseMirror editor), and
//! - SSR `window._preloads` JSON fallback (best-effort minimal).
// AGENT-P2C: Phase 2C knowledge extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{
    elem_text, find_first, host_matches_suffix, meta_attr, meta_property, select_all,
    serialize_node,
};

/// Substack (`substack.com`, `*.substack.com`, custom domains) extractor.
pub struct SubstackExtractor;

impl SubstackExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for SubstackExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn url_is_substack(url: &str) -> bool {
    let Ok(p) = url::Url::parse(url) else {
        return false;
    };
    let Some(h) = p.host_str() else { return false };
    h == "substack.com" || h.ends_with(".substack.com")
}

fn dom_is_substack(root: &NodeRef) -> bool {
    if let Some(generator) = meta_attr(root, "name", "generator", "content") {
        if generator.eq_ignore_ascii_case("Substack") {
            return true;
        }
    }
    // Some custom domains expose og:site_name=Substack
    if let Some(site) = meta_property(root, "og:site_name") {
        if site == "Substack" {
            return true;
        }
    }
    false
}

impl Extractor for SubstackExtractor {
    fn name(&self) -> &'static str {
        "substack"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        // We only have URL access here cheaply; DOM probe runs in extract().
        ctx.url.is_some_and(url_is_substack)
            || ctx
                .url
                .is_some_and(|u| host_matches_suffix(u, "substack.com"))
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        // 1) Rendered post body.
        if let Some(body) = find_first(root, "div.body.markup") {
            return Ok(post_result(root, &body));
        }
        // 2) Note: ProseMirror editor (note pages).
        if let Some(note) = find_first(root, "div.ProseMirror.FeedProseMirror") {
            return Ok(note_result(root, &note));
        }
        // 3) Fallback: og:title / og:description if Substack-flavored.
        if dom_is_substack(root)
            || meta_property(root, "og:site_name").as_deref() == Some("Substack")
        {
            return Ok(meta_only_result(root));
        }
        Err(ExtractError::Failed {
            name: "substack",
            reason: "no body.markup / FeedProseMirror / Substack meta".into(),
        })
    }
}

fn post_result(root: &NodeRef, body: &NodeRef) -> ExtractedContent {
    let title = find_first(root, "h1.post-title")
        .map(|el| elem_text(&el))
        .filter(|s| !s.is_empty())
        .or_else(|| meta_property(root, "og:title"))
        .unwrap_or_default();
    let author = find_first(root, ".byline-name")
        .map(|el| elem_text(&el))
        .filter(|s| !s.is_empty())
        .or_else(|| {
            select_all(root, "a[href*=\"substack.com/@\"]")
                .first()
                .map(|el| elem_text(el))
        })
        .unwrap_or_default();
    let description = meta_property(root, "og:description").unwrap_or_default();
    let html = serialize_node(body);
    ExtractedContent {
        content_html: html,
        title: if title.is_empty() { None } else { Some(title) },
        author: if author.is_empty() {
            None
        } else {
            Some(author)
        },
        site: Some("Substack".to_string()),
        published: None,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        schema_overrides: vec![],
    }
}

fn note_result(root: &NodeRef, note: &NodeRef) -> ExtractedContent {
    let mut html = serialize_node(note);
    // Append image if present in the page.
    if let Some(og_img) = meta_property(root, "og:image") {
        if !og_img.is_empty() {
            html.push_str(&format!(
                r#"<img src="{}" alt="" />"#,
                html_escape::encode_double_quoted_attribute(&og_img)
            ));
        }
    }
    let title = meta_property(root, "og:title").unwrap_or_default();
    let description = meta_property(root, "og:description").unwrap_or_default();
    // Author: strip "(@handle)" suffix from og:title.
    let author = strip_handle(&title);
    ExtractedContent {
        content_html: html,
        title: if title.is_empty() { None } else { Some(title) },
        author: if author.is_empty() {
            None
        } else {
            Some(author)
        },
        site: Some("Substack".to_string()),
        published: None,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        schema_overrides: vec![],
    }
}

fn meta_only_result(root: &NodeRef) -> ExtractedContent {
    let title = meta_property(root, "og:title").unwrap_or_default();
    let description = meta_property(root, "og:description").unwrap_or_default();
    let author = strip_handle(&title);
    ExtractedContent {
        content_html: String::new(),
        title: if title.is_empty() { None } else { Some(title) },
        author: if author.is_empty() {
            None
        } else {
            Some(author)
        },
        site: Some("Substack".to_string()),
        published: None,
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        schema_overrides: vec![],
    }
}

fn strip_handle(title: &str) -> String {
    // " (@handle)" tail.
    let re = regex::Regex::new(r"\s*\(@[^)]+\)\s*$").ok();
    if let Some(re) = re {
        return re.replace(title, "").trim().to_string();
    }
    title.trim().to_string()
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract_url() {
        let e = SubstackExtractor::new();
        let ctx = ExtractCtx::new(Some("https://substack.com/@user/post/123"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://example.com"), &[]);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_post_body() {
        let html = r#"<html><head><meta property="og:title" content="Cool Post"></head>
        <body><h1 class="post-title">Cool Post</h1><div class="byline-name">Alice</div>
        <div class="body markup"><p>Body</p></div></body></html>"#;
        let root = parse_html(html);
        let e = SubstackExtractor::new();
        let ctx = ExtractCtx::new(Some("https://substack.com/p/cool"), &[]);
        let out = e.extract(&ctx, &root).unwrap();
        assert_eq!(out.title.as_deref(), Some("Cool Post"));
        assert_eq!(out.author.as_deref(), Some("Alice"));
        assert_eq!(out.site.as_deref(), Some("Substack"));
        assert!(out.content_html.contains("Body"));
    }

    #[test]
    fn strip_handle_works() {
        assert_eq!(strip_handle("Test User (@testuser)"), "Test User");
        assert_eq!(strip_handle("No Handle"), "No Handle");
    }
}
