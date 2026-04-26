//! Site-specific extractors module.
//!
//! Round-3 ports the full Defuddle extractor suite. The registration order in
//! [`register_defaults`] follows the priority rules in the doc-comment of
//! [`crate::extractor::ExtractorRegistry`]: more-specific matchers register
//! before more-general ones, with the catch-all `BbcodeDataExtractor` last.

use crate::extractor::ExtractorRegistry;
use kuchikiki::NodeRef;

// AGENT-P2B: social-timeline extractors.
mod bluesky;
mod discourse;
mod linkedin;
mod mastodon;
mod reddit;
mod threads;
mod twitter;
mod x_article;
mod x_oembed;

// AGENT-P2A: AI-chat extractors.
mod chatgpt;
mod claude;
mod gemini;
mod grok;

// AGENT-P2C: news / knowledge / dev extractors.
mod c2_wiki;
mod github;
mod hackernews;
mod leetcode;
mod lwn;
mod medium;
mod nytimes;
mod substack;
mod wikipedia;

// AGENT-P2D: YouTube + BBCodeData (catch-all).
mod bbcode_data;
mod youtube;

/// Populate `reg` with the default site-specific extractors in priority
/// order. Returns the registry so callers can chain in builder style.
#[must_use]
pub fn register_defaults(mut reg: ExtractorRegistry) -> ExtractorRegistry {
    // Order from track-e §4 priority table.

    // Social: X/Twitter family — X-Article wins over Twitter wins over X-Oembed.
    reg.register(Box::new(x_article::XArticleExtractor::new()));
    reg.register(Box::new(twitter::TwitterExtractor::new()));
    reg.register(Box::new(x_oembed::XOembedExtractor::new()));

    // Reddit comment threads.
    reg.register(Box::new(reddit::RedditExtractor::new()));

    // YouTube (prefers async; sync path falls back to description + chapters).
    reg.register(Box::new(youtube::YoutubeExtractor::new()));

    // HackerNews (must register before generic news fallbacks).
    reg.register(Box::new(hackernews::HackerNewsExtractor::new()));

    // AI chat assistants.
    reg.register(Box::new(chatgpt::ChatGptExtractor::new()));
    reg.register(Box::new(claude::ClaudeExtractor::new()));
    reg.register(Box::new(grok::GrokExtractor::new()));
    reg.register(Box::new(gemini::GeminiExtractor::new()));

    // Dev / code-hosting.
    reg.register(Box::new(github::GitHubExtractor::new()));
    reg.register(Box::new(linkedin::LinkedInExtractor::new()));

    // Other social timelines.
    reg.register(Box::new(threads::ThreadsExtractor::new()));
    reg.register(Box::new(bluesky::BlueskyExtractor::new()));

    // News / knowledge family.
    reg.register(Box::new(medium::MediumExtractor::new()));
    reg.register(Box::new(c2_wiki::C2WikiExtractor::new()));
    reg.register(Box::new(substack::SubstackExtractor::new()));
    reg.register(Box::new(nytimes::NytimesExtractor::new()));
    reg.register(Box::new(wikipedia::WikipediaExtractor::new()));

    // Generator-meta-matched (Mastodon, Discourse).
    reg.register(Box::new(mastodon::MastodonExtractor::new()));
    reg.register(Box::new(discourse::DiscourseExtractor::new()));

    // Long-tail dev / publishing.
    reg.register(Box::new(leetcode::LeetCodeExtractor::new()));
    reg.register(Box::new(lwn::LwnExtractor::new()));

    // Catch-all (BBCode-detection across any host) registered LAST.
    reg.register(Box::new(bbcode_data::BbcodeDataExtractor::new()));

    reg
}

// ---------------------------------------------------------------------------
// Shared helpers used by extractors. crate-private; not part of the public API.
// ---------------------------------------------------------------------------

/// True if the URL host equals `target` (with optional `www.` prefix).
pub(crate) fn host_matches_exact(url: &str, target: &str) -> bool {
    let Ok(p) = url::Url::parse(url) else {
        return false;
    };
    let Some(h) = p.host_str() else { return false };
    let h = h.strip_prefix("www.").unwrap_or(h);
    h == target
}

/// True if the URL host ends with `suffix` (counts subdomains too).
pub(crate) fn host_matches_suffix(url: &str, suffix: &str) -> bool {
    let Ok(p) = url::Url::parse(url) else {
        return false;
    };
    let Some(h) = p.host_str() else { return false };
    let h = h.strip_prefix("www.").unwrap_or(h);
    h == suffix || h.ends_with(&format!(".{suffix}"))
}

/// First match for `selector` under `root`. Returns `None` on selector
/// parse error too — extractors that want to bubble that should call
/// `root.select(...)` themselves.
pub(crate) fn find_first(root: &NodeRef, selector: &str) -> Option<NodeRef> {
    root.select_first(selector)
        .ok()
        .map(|d| d.as_node().clone())
}

/// Like [`find_first`] but rooted at an element node rather than the document.
pub(crate) fn find_first_in(parent: &NodeRef, selector: &str) -> Option<NodeRef> {
    parent
        .select_first(selector)
        .ok()
        .map(|d| d.as_node().clone())
}

/// All matches for `selector` under `root`.
pub(crate) fn select_all(root: &NodeRef, selector: &str) -> Vec<NodeRef> {
    match root.select(selector) {
        Ok(iter) => iter.map(|d| d.as_node().clone()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Detach every matching element from the tree.
pub(crate) fn remove_all(root: &NodeRef, selector: &str) {
    let nodes = select_all(root, selector);
    for n in nodes {
        n.detach();
    }
}

/// Get an attribute value from an element node.
pub(crate) fn elem_attr(node: &NodeRef, name: &str) -> Option<String> {
    node.as_element()
        .and_then(|el| el.attributes.borrow().get(name).map(str::to_string))
}

/// Collect this element's text content.
pub(crate) fn elem_text(node: &NodeRef) -> String {
    let mut s = String::new();
    for child in node.descendants() {
        if let Some(t) = child.as_text() {
            s.push_str(&t.borrow());
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Serialize the children of a node (no opening/closing tag of `node` itself).
pub(crate) fn serialize_children(node: &NodeRef) -> String {
    let mut buf: Vec<u8> = Vec::new();
    for child in node.children() {
        let _ = child.serialize(&mut buf);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// Serialize a node including its own opening/closing tag.
pub(crate) fn serialize_node(node: &NodeRef) -> String {
    let mut buf: Vec<u8> = Vec::new();
    let _ = node.serialize(&mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Read a `<meta property="..." content="...">` tag.
pub(crate) fn meta_property(root: &NodeRef, prop: &str) -> Option<String> {
    let nodes = select_all(root, &format!("meta[property=\"{prop}\"]"));
    for n in nodes {
        if let Some(v) = elem_attr(&n, "content") {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Read a `<meta name="..." content="...">` tag.
pub(crate) fn meta_name(root: &NodeRef, name: &str) -> Option<String> {
    let nodes = select_all(root, &format!("meta[name=\"{name}\"]"));
    for n in nodes {
        if let Some(v) = elem_attr(&n, "content") {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// Generic `<meta>` lookup used by the P2C extractors:
/// finds the first `<meta {attr_name}="{attr_value}">` and returns its
/// `{value_attr}` value. Most callers pass `"name"`, the meta key, and
/// `"content"`.
pub(crate) fn meta_attr(
    root: &NodeRef,
    attr_name: &str,
    attr_value: &str,
    value_attr: &str,
) -> Option<String> {
    let nodes = select_all(root, &format!("meta[{attr_name}=\"{attr_value}\"]"));
    for n in nodes {
        if let Some(v) = elem_attr(&n, value_attr) {
            if !v.trim().is_empty() {
                return Some(v);
            }
        }
    }
    None
}

/// HTML-escape text content suitable for putting between tags. `<>&` are
/// rewritten; quotes are passed through.
pub(crate) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            c => out.push(c),
        }
    }
    out
}

/// Escape an attribute value for use inside `key="..."` — additionally
/// escapes the double-quote.
pub(crate) fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
    out
}
