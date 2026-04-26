//! LWN.net article extractor — port of Defuddle's `lwn.ts`.
//!
//! Triggers on the presence of `.PageHeadline` and `.ArticleText`.
//! Extracts article body from `.ArticleText main` (with comment forms /
//! anchors stripped) plus a flat comment tree from `details.CommentBox`.
// AGENT-P2C: Phase 2C news extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{
    elem_text, escape_html, find_first, find_first_in, host_matches_exact, select_all,
    serialize_children, serialize_node,
};

/// LWN.net (`lwn.net`) article extractor.
pub struct LwnExtractor;

impl LwnExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for LwnExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for LwnExtractor {
    fn name(&self) -> &'static str {
        "lwn"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        if !ctx.url.is_some_and(|u| host_matches_exact(u, "lwn.net")) {
            return false;
        }
        true
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        if find_first(root, ".PageHeadline").is_none() || find_first(root, ".ArticleText").is_none()
        {
            return Err(ExtractError::Failed {
                name: "lwn",
                reason: "missing PageHeadline / ArticleText".into(),
            });
        }
        let title = find_first(root, ".PageHeadline h1")
            .map(|el| elem_text(&el))
            .unwrap_or_default();
        let byline = find_first(root, ".Byline")
            .map(|el| elem_text(&el))
            .unwrap_or_default();
        let author = parse_author(&byline);
        let published = parse_date(&byline);
        let description =
            crate::extractors::meta_property(root, "og:description").unwrap_or_default();

        let main =
            find_first(root, ".ArticleText main").or_else(|| find_first(root, ".ArticleText"));
        let article_html = main.as_ref().map(serialize_children).unwrap_or_default();
        let comments_html = main.as_ref().map(extract_comments).unwrap_or_default();

        let mut full = article_html;
        if !comments_html.is_empty() {
            full.push_str("<hr><h2>Comments</h2>");
            full.push_str(&comments_html);
        }

        Ok(ExtractedContent {
            content_html: full,
            title: if title.is_empty() { None } else { Some(title) },
            author: if author.is_empty() {
                None
            } else {
                Some(author)
            },
            site: Some("LWN.net".to_string()),
            published: if published.is_empty() {
                None
            } else {
                Some(published)
            },
            description: if description.is_empty() {
                None
            } else {
                Some(description)
            },
            schema_overrides: vec![],
        })
    }
}

fn parse_author(byline: &str) -> String {
    // "by <name>"
    let lc = byline.to_ascii_lowercase();
    if let Some(idx) = lc.find("by ") {
        let rest = &byline[idx + 3..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        return name;
    }
    String::new()
}

fn parse_date(text: &str) -> String {
    // "Posted Mar 12, 2025"
    let re = regex::Regex::new(r"Posted\s+(\w+)\s+(\d+),\s+(\d{4})").ok();
    let Some(re) = re else { return String::new() };
    let Some(c) = re.captures(text) else {
        return String::new();
    };
    let month = month_to_num(&c[1]);
    let day = c[2].parse::<u32>().unwrap_or(0);
    let year = &c[3];
    if month.is_empty() || day == 0 {
        return String::new();
    }
    format!("{year}-{month}-{day:02}")
}

fn month_to_num(m: &str) -> &'static str {
    match &m.to_ascii_lowercase()[..3.min(m.len())] {
        "jan" => "01",
        "feb" => "02",
        "mar" => "03",
        "apr" => "04",
        "may" => "05",
        "jun" => "06",
        "jul" => "07",
        "aug" => "08",
        "sep" => "09",
        "oct" => "10",
        "nov" => "11",
        "dec" => "12",
        _ => "",
    }
}

fn extract_comments(main: &NodeRef) -> String {
    let boxes = select_all(main, "details.CommentBox");
    if boxes.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for b in &boxes {
        let depth = comment_depth(b, main);
        let poster_text = find_first_in(b, ".CommentPoster")
            .map(|p| elem_text(&p))
            .unwrap_or_default();
        let author: String = poster_text
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string();
        let date = parse_date(&poster_text);
        let formatted = find_first_in(b, ".FormattedComment");
        let body = formatted.as_ref().map(serialize_node).unwrap_or_default();
        for _ in 0..depth {
            out.push_str("<blockquote>");
        }
        out.push_str("<blockquote>");
        out.push_str(&format!("<p><strong>{}</strong>", escape_html(&author)));
        if !date.is_empty() {
            out.push_str(&format!(" · {}", escape_html(&date)));
        }
        out.push_str("</p>");
        out.push_str(&body);
        out.push_str("</blockquote>");
        for _ in 0..depth {
            out.push_str("</blockquote>");
        }
    }
    out
}

fn comment_depth(node: &NodeRef, root: &NodeRef) -> u32 {
    let mut depth = 0u32;
    let mut cur = node.parent();
    while let Some(p) = cur {
        if std::rc::Rc::ptr_eq(&p.0, &root.0) {
            break;
        }
        if let Some(el) = p.as_element() {
            let attrs = el.attributes.borrow();
            if el.name.local.as_ref().eq_ignore_ascii_case("details") {
                let class = attrs.get("class").unwrap_or("");
                if class.split_whitespace().any(|c| c == "CommentBox") {
                    depth += 1;
                }
            }
        }
        cur = p.parent();
    }
    depth
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract() {
        let e = LwnExtractor::new();
        let ctx = ExtractCtx::new(Some("https://lwn.net/Articles/123/"), &[]);
        let html = r#"<html><body><div class="PageHeadline"><h1>X</h1></div><div class="ArticleText"><main><p>Body</p></main></div></body></html>"#;
        let root = parse_html(html);
        assert!(e.can_extract(&ctx));
        let out = e.extract(&ctx, &root).unwrap();
        assert_eq!(out.title.as_deref(), Some("X"));
        assert_eq!(out.site.as_deref(), Some("LWN.net"));
        assert!(out.content_html.contains("Body"));
    }

    #[test]
    fn parse_byline_date() {
        assert_eq!(parse_author("by alice on something"), "alice");
        assert_eq!(parse_date("Posted Mar 12, 2025"), "2025-03-12");
    }
}
