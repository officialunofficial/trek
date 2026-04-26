//! GitHub extractor — port of Defuddle's `github.ts`.
//!
//! Recognises GitHub by URL host plus DOM markers (`meta[name=octolytics-url]`,
//! `.js-header-wrapper`, etc.). Three branches:
//! - Issue page: extract from `[data-testid="issue-viewer-issue-container"]`.
//! - PR page: extract from `[id^="pullrequest-"]` + `.timeline-comment`.
//! - README / repo-root: extract from `<article class="markdown-body">`.
// AGENT-P2C: Phase 2C dev extractor.

use kuchikiki::NodeRef;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{
    elem_attr, elem_text, escape_html, find_first, find_first_in, host_matches_exact, meta_attr,
    select_all, serialize_node,
};

/// GitHub (`github.com`) extractor.
pub struct GitHubExtractor;

impl GitHubExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for GitHubExtractor {
    fn default() -> Self {
        Self::new()
    }
}

fn is_github_dom(root: &NodeRef) -> bool {
    let probes: &[(&str, &str, &str, &str)] = &[
        ("meta", "name", "octolytics-url", "content"),
        ("meta", "name", "github-keyboard-shortcuts", "content"),
    ];
    for (_t, k, v, _attr) in probes {
        if meta_attr(root, k, v, "content").is_some() {
            return true;
        }
    }
    if find_first(root, ".js-header-wrapper").is_some() {
        return true;
    }
    if find_first(root, "#js-repo-pjax-container").is_some() {
        return true;
    }
    false
}

fn classify(url: &str) -> Kind {
    if regex::Regex::new(r"/issues/\d+").unwrap().is_match(url) {
        Kind::Issue
    } else if regex::Regex::new(r"/pull/\d+").unwrap().is_match(url) {
        Kind::Pr
    } else {
        Kind::Repo
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Issue,
    Pr,
    Repo,
}

impl Extractor for GitHubExtractor {
    fn name(&self) -> &'static str {
        "github"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url.is_some_and(|u| host_matches_exact(u, "github.com"))
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        if !is_github_dom(root) {
            return Err(ExtractError::Failed {
                name: "github",
                reason: "no GitHub DOM markers".into(),
            });
        }
        let url = ctx.url.unwrap_or_default();
        let kind = classify(url);
        let (owner, repo) = repo_info(url, root);
        let number = extract_number(url, root);
        let site = if owner.is_empty() {
            "GitHub".to_string()
        } else {
            format!("GitHub - {owner}/{repo}")
        };

        let (content, author, published) = match kind {
            Kind::Issue => extract_issue(root),
            Kind::Pr => extract_pr(root),
            Kind::Repo => extract_repo(root),
        };
        if content.is_empty() {
            return Err(ExtractError::Failed {
                name: "github",
                reason: "no body extracted".into(),
            });
        }
        let title = find_first(root, "title")
            .map(|t| elem_text(&t))
            .filter(|s| !s.is_empty());

        Ok(ExtractedContent {
            content_html: content,
            title,
            author: if author.is_empty() {
                None
            } else {
                Some(author)
            },
            site: Some(site),
            published: if published.is_empty() {
                None
            } else {
                Some(published)
            },
            description: None,
            schema_overrides: vec![if number.is_empty() {
                serde_json::json!({})
            } else {
                serde_json::json!({"@type": "DiscussionForumPosting", "identifier": number})
            }],
        })
    }
}

fn repo_info(url: &str, _root: &NodeRef) -> (String, String) {
    let re = regex::Regex::new(r"github\.com/([^/]+)/([^/?#]+)").unwrap();
    if let Some(c) = re.captures(url) {
        return (c[1].to_string(), c[2].to_string());
    }
    (String::new(), String::new())
}

fn extract_number(url: &str, _root: &NodeRef) -> String {
    let re = regex::Regex::new(r"/(?:issues|pull)/(\d+)").unwrap();
    re.captures(url)
        .map(|c| c[1].to_string())
        .unwrap_or_default()
}

fn extract_issue(root: &NodeRef) -> (String, String, String) {
    let container = find_first(root, "[data-testid=\"issue-viewer-issue-container\"]");
    let body_el = container
        .as_ref()
        .and_then(|c| find_first_in(c, "[data-testid=\"issue-body-viewer\"] .markdown-body"))
        .or_else(|| find_first(root, "[data-testid=\"issue-body\"]"))
        .or_else(|| find_first(root, ".markdown-body"));
    let content = body_el.as_ref().map(serialize_node).unwrap_or_default();
    let author = container
        .as_ref()
        .and_then(|c| find_first_in(c, "a[data-testid=\"issue-body-header-author\"]"))
        .or_else(|| find_first(root, "a[data-testid=\"avatar-link\"]"))
        .map(|el| {
            elem_attr(&el, "href")
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string()
        })
        .unwrap_or_default();
    let published = container
        .as_ref()
        .and_then(|c| find_first_in(c, "relative-time"))
        .or_else(|| find_first(root, "relative-time"))
        .and_then(|el| elem_attr(&el, "datetime"))
        .unwrap_or_default();

    let comments = extract_comments_generic(root);
    let mut full = content;
    if !comments.is_empty() {
        full.push_str("<hr><h2>Comments</h2>");
        full.push_str(&comments);
    }
    (full, author, published)
}

fn extract_pr(root: &NodeRef) -> (String, String, String) {
    let pr_body = find_first(root, "[id^=\"pullrequest-\"]")
        .or_else(|| find_first(root, ".timeline-comment"));
    let body_el = pr_body
        .as_ref()
        .and_then(|p| find_first_in(p, ".comment-body.markdown-body"))
        .or_else(|| find_first(root, ".comment-body.markdown-body"))
        .or_else(|| find_first(root, ".markdown-body"));
    let content = body_el.as_ref().map(serialize_node).unwrap_or_default();
    let author = pr_body
        .as_ref()
        .and_then(|p| find_first_in(p, ".author"))
        .or_else(|| find_first(root, ".author"))
        .map(|el| elem_text(&el))
        .unwrap_or_default();
    let published = pr_body
        .as_ref()
        .and_then(|p| find_first_in(p, "relative-time"))
        .or_else(|| find_first(root, "relative-time"))
        .and_then(|el| elem_attr(&el, "datetime"))
        .unwrap_or_default();

    let comments = extract_pr_comments(root, pr_body.as_ref());
    let mut full = content;
    if !comments.is_empty() {
        full.push_str("<hr><h2>Comments</h2>");
        full.push_str(&comments);
    }
    (full, author, published)
}

fn extract_repo(root: &NodeRef) -> (String, String, String) {
    let body =
        find_first(root, "article.markdown-body").or_else(|| find_first(root, ".markdown-body"));
    let content = body.as_ref().map(serialize_node).unwrap_or_default();
    (content, String::new(), String::new())
}

fn extract_comments_generic(root: &NodeRef) -> String {
    let mut out = String::new();
    let comments = select_all(root, "[data-wrapper-timeline-id]");
    for c in &comments {
        let body = match find_first_in(c, ".markdown-body") {
            Some(b) => b,
            None => continue,
        };
        let author = find_first_in(c, "a[data-testid=\"avatar-link\"]")
            .or_else(|| find_first_in(c, "a[href^=\"/\"][data-hovercard-url*=\"/users/\"]"))
            .map(|el| {
                elem_attr(&el, "href")
                    .unwrap_or_default()
                    .trim_start_matches('/')
                    .to_string()
            })
            .unwrap_or_default();
        let date = find_first_in(c, "relative-time")
            .and_then(|t| elem_attr(&t, "datetime"))
            .unwrap_or_default()
            .split('T')
            .next()
            .unwrap_or_default()
            .to_string();
        out.push_str("<blockquote>");
        out.push_str(&format!("<p><strong>{}</strong>", escape_html(&author)));
        if !date.is_empty() {
            out.push_str(&format!(" · {}", escape_html(&date)));
        }
        out.push_str("</p>");
        out.push_str(&serialize_node(&body));
        out.push_str("</blockquote>");
    }
    out
}

fn extract_pr_comments(root: &NodeRef, pr_body: Option<&NodeRef>) -> String {
    let mut out = String::new();
    let comments = select_all(root, ".timeline-comment, .review-comment");
    for c in &comments {
        if let Some(pb) = pr_body {
            // Skip if this *is* the PR body itself.
            if std::rc::Rc::ptr_eq(&c.0, &pb.0) {
                continue;
            }
        }
        let body = match find_first_in(c, ".comment-body.markdown-body") {
            Some(b) => b,
            None => continue,
        };
        let author = find_first_in(c, ".author")
            .map(|el| elem_text(&el))
            .unwrap_or_default();
        let date = find_first_in(c, "relative-time")
            .and_then(|t| elem_attr(&t, "datetime"))
            .unwrap_or_default()
            .split('T')
            .next()
            .unwrap_or_default()
            .to_string();
        out.push_str("<blockquote>");
        out.push_str(&format!("<p><strong>{}</strong>", escape_html(&author)));
        if !date.is_empty() {
            out.push_str(&format!(" · {}", escape_html(&date)));
        }
        out.push_str("</p>");
        out.push_str(&serialize_node(&body));
        out.push_str("</blockquote>");
    }
    out
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract_url() {
        let e = GitHubExtractor::new();
        let ctx = ExtractCtx::new(Some("https://github.com/foo/bar/issues/1"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://example.com"), &[]);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn extracts_pr_body() {
        let html = r#"<html><head><meta name="octolytics-url" content="x"></head><body>
        <div id="pullrequest-42"><div class="comment-body markdown-body"><p>PR description</p></div>
        <span class="author">octocat</span><relative-time datetime="2025-01-15T10:00:00Z"></relative-time>
        </div></body></html>"#;
        let root = parse_html(html);
        let e = GitHubExtractor::new();
        let ctx = ExtractCtx::new(Some("https://github.com/foo/bar/pull/42"), &[]);
        let out = e.extract(&ctx, &root).unwrap();
        assert!(out.content_html.contains("PR description"));
        assert_eq!(out.author.as_deref(), Some("octocat"));
        assert_eq!(out.site.as_deref(), Some("GitHub - foo/bar"));
    }

    #[test]
    fn classifies_url() {
        assert_eq!(classify("https://github.com/x/y/issues/1"), Kind::Issue);
        assert_eq!(classify("https://github.com/x/y/pull/2"), Kind::Pr);
        assert_eq!(classify("https://github.com/x/y"), Kind::Repo);
    }
}
