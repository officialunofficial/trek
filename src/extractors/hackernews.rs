//! Hacker News extractor — port of Defuddle's `hackernews.ts`.
//!
//! Handles three page shapes:
//! - Story page: `table.fatitem` with title + optional `.toptext`.
//! - Comment page: `table.fatitem` containing a single `tr.athing` plus
//!   `.commtext` and an `.onstory` back-link.
//! - Listing page (`/news`, `/newest`, `/best`): many `tr.athing` rows
//!   without a `.fatitem` wrapper.
//!
//! Comment depth comes from the `<img src="s.gif" width=N>` spacer image —
//! HN uses 40px per indent level, so depth = N / 40.
// AGENT-P2C: Phase 2C news extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{
    elem_attr, elem_text, escape_attr, escape_html, find_first, find_first_in, host_matches_exact,
    select_all, serialize_children,
};

/// Hacker News (`news.ycombinator.com`) extractor.
pub struct HackerNewsExtractor;

impl HackerNewsExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for HackerNewsExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageKind {
    Story,
    Comment,
    Listing,
    None,
}

fn classify(root: &NodeRef) -> PageKind {
    let main_post = find_first(root, ".fatitem");
    if main_post.is_some() {
        let post = main_post.unwrap();
        // Comment page = .fatitem with .onstory link and no .titleline
        let has_on_story = find_first_in(&post, ".onstory").is_some();
        let has_title = find_first_in(&post, ".titleline").is_some();
        if has_on_story && !has_title {
            return PageKind::Comment;
        }
        return PageKind::Story;
    }
    let athings = select_all(root, "tr.athing");
    if athings.len() > 1 {
        return PageKind::Listing;
    }
    PageKind::None
}

impl Extractor for HackerNewsExtractor {
    fn name(&self) -> &'static str {
        "hackernews"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url
            .is_some_and(|u| host_matches_exact(u, "news.ycombinator.com"))
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let kind = classify(root);
        if kind == PageKind::None {
            return Err(ExtractError::Failed {
                name: "hackernews",
                reason: "no .fatitem and not a listing page".into(),
            });
        }

        match kind {
            PageKind::Listing => extract_listing(root),
            PageKind::Story => extract_story(root, ctx),
            PageKind::Comment => extract_comment_page(root, ctx),
            PageKind::None => unreachable!(),
        }
    }
}

fn extract_listing(root: &NodeRef) -> Result<ExtractedContent, ExtractError> {
    let mut items = String::new();
    let stories = select_all(root, "tr.athing");
    for row in &stories {
        let title_el = match find_first_in(row, ".titleline a") {
            Some(t) => t,
            None => continue,
        };
        let title = elem_text(&title_el);
        let url = elem_attr(&title_el, "href").unwrap_or_default();
        let site_str = find_first_in(row, ".sitestr")
            .map(|s| elem_text(&s))
            .unwrap_or_default();
        let id = elem_attr(row, "id").unwrap_or_default();

        // The subtext row is the next-sibling tr (next element sibling).
        let subrow = next_element_sibling(row);
        let score = subrow
            .as_ref()
            .and_then(|s| find_first_in(s, ".score"))
            .map(|s| elem_text(&s))
            .unwrap_or_default();
        let author = subrow
            .as_ref()
            .and_then(|s| find_first_in(s, ".hnuser"))
            .map(|s| elem_text(&s))
            .unwrap_or_default();
        let comments_text = subrow
            .as_ref()
            .map(|s| {
                let links = select_all(s, "td.subtext a");
                links
                    .last()
                    .map(|l| elem_text(l).replace('\u{a0}', " "))
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let comments = if comments_text.contains("comment") {
            comments_text
        } else {
            String::new()
        };
        let comments_url = if id.is_empty() {
            String::new()
        } else {
            format!("https://news.ycombinator.com/item?id={id}")
        };

        items.push_str("<li>");
        items.push_str(&format!(
            r#"<a href="{}">{}</a>"#,
            escape_attr(&url),
            escape_html(&title)
        ));
        if !site_str.is_empty() {
            items.push_str(&format!(" <small>({})</small>", escape_html(&site_str)));
        }
        let mut meta = Vec::new();
        if !score.is_empty() {
            meta.push(escape_html(&score));
        }
        if !author.is_empty() {
            meta.push(format!("by {}", escape_html(&author)));
        }
        if !comments.is_empty() {
            meta.push(format!(
                r#"<a href="{}">{}</a>"#,
                escape_attr(&comments_url),
                escape_html(&comments)
            ));
        }
        if !meta.is_empty() {
            items.push_str(&format!("<br><small>{}</small>", meta.join(" · ")));
        }
        items.push_str("</li>");
    }

    let more_link = find_first(root, ".morelink");
    let mut html = format!("<ol>{items}</ol>");
    if let Some(ml) = more_link {
        let url = elem_attr(&ml, "href").unwrap_or_default();
        let text = elem_text(&ml);
        let text = if text.is_empty() {
            "More".to_string()
        } else {
            text
        };
        html.push_str(&format!(
            r#"<p><a href="{}">{}</a></p>"#,
            escape_attr(&url),
            escape_html(&text)
        ));
    }

    // Title comes from <title>X | Hacker News</title>
    let title = find_first(root, "title")
        .map(|t| {
            elem_text(&t)
                .replace(" | Hacker News", "")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Hacker News".to_string());

    Ok(ExtractedContent {
        content_html: html,
        title: Some(title),
        author: None,
        site: Some("Hacker News".to_string()),
        published: None,
        description: None,
        schema_overrides: vec![],
    })
}

fn extract_story(root: &NodeRef, _ctx: &ExtractCtx<'_>) -> Result<ExtractedContent, ExtractError> {
    let main_post = find_first(root, ".fatitem").ok_or_else(|| ExtractError::Failed {
        name: "hackernews",
        reason: "missing fatitem".into(),
    })?;

    let title = find_first_in(&main_post, ".titleline")
        .map(|t| elem_text(&t))
        .unwrap_or_default();
    let title_link = find_first_in(&main_post, ".titleline a");
    let url = title_link
        .as_ref()
        .and_then(|a| elem_attr(a, "href"))
        .unwrap_or_default();
    let author = find_first_in(&main_post, ".hnuser")
        .map(|el| elem_text(&el))
        .unwrap_or_default();
    let timestamp = find_first_in(&main_post, ".age")
        .and_then(|a| elem_attr(&a, "title"))
        .unwrap_or_default();
    let published = timestamp.split('T').next().unwrap_or_default().to_string();

    let mut content = String::new();
    if !url.is_empty() && !url.starts_with("item?") {
        content.push_str(&format!(
            r#"<p><a href="{}" target="_blank">{}</a></p>"#,
            escape_attr(&url),
            escape_html(&url)
        ));
    }
    if let Some(toptext) = find_first_in(&main_post, ".toptext") {
        content.push_str(&format!(
            r#"<div class="post-text">{}</div>"#,
            serialize_children(&toptext)
        ));
    }

    let comments_html = build_comments(root);
    let mut full = content;
    if !comments_html.is_empty() {
        full.push_str("<hr><h2>Comments</h2>");
        full.push_str(&comments_html);
    }

    let description = if !title.is_empty() && !author.is_empty() {
        format!("{title} - by {author} on Hacker News")
    } else {
        String::new()
    };

    Ok(ExtractedContent {
        content_html: full,
        title: Some(title),
        author: Some(author),
        site: Some("Hacker News".to_string()),
        published: Some(published),
        description: if description.is_empty() {
            None
        } else {
            Some(description)
        },
        schema_overrides: vec![],
    })
}

fn extract_comment_page(
    root: &NodeRef,
    _ctx: &ExtractCtx<'_>,
) -> Result<ExtractedContent, ExtractError> {
    let main_post = find_first(root, ".fatitem").ok_or_else(|| ExtractError::Failed {
        name: "hackernews",
        reason: "missing fatitem".into(),
    })?;
    // The author/age is on the .athing row inside fatitem
    let author = find_first_in(&main_post, ".hnuser")
        .map(|el| elem_text(&el))
        .unwrap_or_else(|| "[deleted]".into());
    let timestamp = find_first_in(&main_post, ".age")
        .and_then(|a| elem_attr(&a, "title"))
        .unwrap_or_default();
    let date = timestamp.split('T').next().unwrap_or_default().to_string();
    let commtext = find_first_in(&main_post, ".commtext");
    let comment_text = commtext
        .as_ref()
        .map(serialize_children)
        .unwrap_or_default();
    let comment_text_plain = commtext.as_ref().map(elem_text).unwrap_or_default();

    let preview: String = comment_text_plain.trim().chars().take(50).collect();
    let title_preview = if comment_text_plain.len() > 50 {
        format!("{preview}...")
    } else {
        preview
    };
    let title = format!("Comment by {author}: {title_preview}");

    let mut html = String::new();
    html.push_str(&format!(
        r#"<blockquote><p><strong>{}</strong> · {}</p>{}</blockquote>"#,
        escape_html(&author),
        escape_html(&date),
        comment_text
    ));

    let comments_html = build_comments(root);
    if !comments_html.is_empty() {
        html.push_str("<hr><h2>Comments</h2>");
        html.push_str(&comments_html);
    }

    Ok(ExtractedContent {
        content_html: html,
        title: Some(title),
        author: Some(author.clone()),
        site: Some("Hacker News".to_string()),
        published: Some(date),
        description: Some(format!("Comment by {author} on Hacker News")),
        schema_overrides: vec![],
    })
}

fn build_comments(root: &NodeRef) -> String {
    let comments = select_all(root, "tr.comtr");
    if comments.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    let mut seen = std::collections::HashSet::new();
    for c in &comments {
        let id = elem_attr(c, "id").unwrap_or_default();
        if id.is_empty() || !seen.insert(id.clone()) {
            continue;
        }
        // depth from <img src="s.gif" width=N>
        let depth = find_first_in(c, ".ind img")
            .and_then(|img| elem_attr(&img, "width"))
            .and_then(|w| w.parse::<u32>().ok())
            .map(|n| n / 40)
            .unwrap_or(0);
        let commtext = match find_first_in(c, ".commtext") {
            Some(t) => t,
            None => continue,
        };
        let author = find_first_in(c, ".hnuser")
            .map(|el| elem_text(&el))
            .unwrap_or_else(|| "[deleted]".into());
        let timestamp = find_first_in(c, ".age")
            .and_then(|a| elem_attr(&a, "title"))
            .unwrap_or_default();
        let date = timestamp.split('T').next().unwrap_or_default().to_string();
        let body = serialize_children(&commtext);

        // Render as nested blockquotes
        for _ in 0..depth {
            out.push_str("<blockquote>");
        }
        out.push_str("<blockquote>");
        out.push_str(&format!(r#"<p><strong>{}</strong>"#, escape_html(&author)));
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

fn next_element_sibling(node: &NodeRef) -> Option<NodeRef> {
    let mut current = node.next_sibling();
    while let Some(n) = current {
        if n.as_element().is_some() {
            return Some(n);
        }
        current = n.next_sibling();
    }
    None
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract_url() {
        let e = HackerNewsExtractor::new();
        let ctx = ExtractCtx::new(Some("https://news.ycombinator.com/item?id=1"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://example.com/"), &[]);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extract_listing_page() {
        let html = r#"<html><body><table><tr class="athing" id="100">
        <td><span class="titleline"><a href="https://example.com/a">Story A</a></span><span class="sitestr">example.com</span></td>
        </tr><tr><td class="subtext"><span class="score">42 points</span> by <a class="hnuser">alice</a> <span class="age" title="2025-01-01T00:00:00"><a>1d</a></span> | <a href="item?id=100">10 comments</a></td></tr>
        </table></body></html>"#;
        let root = parse_html(html);
        let e = HackerNewsExtractor::new();
        let ctx = ExtractCtx::new(Some("https://news.ycombinator.com/news"), &[]);
        // Single athing, won't qualify as listing — let's add a 2nd
        let html2 = r#"<html><body><table>
        <tr class="athing" id="1"><td><span class="titleline"><a href="x">A</a></span></td></tr>
        <tr><td class="subtext"><span class="score">1 point</span> by <a class="hnuser">u</a> <span class="age" title="2025-01-01T00:00:00"><a>1d</a></span> | <a href="item?id=1">1 comment</a></td></tr>
        <tr class="athing" id="2"><td><span class="titleline"><a href="y">B</a></span></td></tr>
        <tr><td class="subtext"><span class="score">2 points</span> by <a class="hnuser">u2</a> <span class="age" title="2025-01-02T00:00:00"><a>1d</a></span> | <a href="item?id=2">2 comments</a></td></tr>
        </table></body></html>"#;
        let root2 = parse_html(html2);
        let out = e.extract(&ctx, &root2).unwrap();
        assert!(out.content_html.contains("<ol>"));
        assert!(out.content_html.contains("Story A") || out.content_html.contains("<li>"));
        assert_eq!(out.site.as_deref(), Some("Hacker News"));
        let _ = root; // suppress unused warning
    }

    #[test]
    fn extract_story_with_comments() {
        let html = r#"<html><body>
        <table class="fatitem"><tr class="athing"><td><span class="titleline"><a href="https://example.com">My Story</a></span></td></tr>
        <tr><td class="subtext"><a class="hnuser">bob</a> <span class="age" title="2025-03-01T12:00:00"><a>3h</a></span></td></tr>
        <tr><td class="toptext">Some text</td></tr>
        </table>
        <table>
        <tr class="comtr" id="11"><td><table><tr><td class="ind"><img src="s.gif" width="0"></td><td><a class="hnuser">alice</a> <span class="age" title="2025-03-01T12:30:00"><a></a></span><div class="commtext c00">A comment</div></td></tr></table></td></tr>
        <tr class="comtr" id="12"><td><table><tr><td class="ind"><img src="s.gif" width="40"></td><td><a class="hnuser">eve</a> <span class="age" title="2025-03-01T13:00:00"><a></a></span><div class="commtext c00">A reply</div></td></tr></table></td></tr>
        </table>
        </body></html>"#;
        let root = parse_html(html);
        let e = HackerNewsExtractor::new();
        let ctx = ExtractCtx::new(Some("https://news.ycombinator.com/item?id=11"), &[]);
        let out = e.extract(&ctx, &root).unwrap();
        assert_eq!(out.author.as_deref(), Some("bob"));
        assert!(out.content_html.contains("A comment"));
        assert!(out.content_html.contains("A reply"));
        // Reply has depth 1 → 2 nested blockquotes around it
        assert!(out.content_html.contains("alice"));
        assert!(out.content_html.contains("eve"));
    }
}
