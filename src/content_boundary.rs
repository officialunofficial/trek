//! Locate the start of the article prose body.
//!
//! Port of `defuddle/content-boundary.ts::findContentStart`. Used by removal
//! passes that want to scope themselves to "above the article body."

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::dom::walk::{
    closest_tag, count_words, get_attr, is_any_tag, link_text_length, text_content,
};

const PROSE_MIN_WORDS: usize = 7;
static SENTENCE_PUNCT: Lazy<Regex> = Lazy::new(|| Regex::new(r"[.!?]").expect("valid regex"));
static BYLINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)^by\s+\S").expect("valid regex"));
static DATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}|\d{1,2}(?:st|nd|rd|th)?\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)|\d{4}[-/]\d{1,2}[-/]\d{1,2}").expect("valid regex")
});

const SKIP_ANCESTOR_TAGS: &[&str] = &["aside", "nav", "header", "footer", "form"];

fn normalize_text(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn find_title_element(root: &NodeRef, title: &str) -> Option<NodeRef> {
    let normalized = normalize_text(title);
    if normalized.is_empty() {
        return None;
    }
    for d in root.descendants() {
        if is_any_tag(&d, &["h1", "h2"]) {
            if normalize_text(&text_content(&d)) == normalized {
                return Some(d);
            }
        }
    }
    None
}

fn is_prose_block(node: &NodeRef) -> bool {
    if !is_any_tag(
        node,
        &["p", "div", "section", "article", "blockquote", "font"],
    ) {
        return false;
    }
    if closest_tag(node, SKIP_ANCESTOR_TAGS).is_some() {
        return false;
    }
    if let Some(class) = get_attr(node, "class") {
        if class.contains("isHidden") || class.contains("is-hidden") {
            return false;
        }
    }
    let txt = text_content(node);
    let txt = txt.trim();
    if txt.is_empty() {
        return false;
    }
    let words = count_words(txt);
    if words < PROSE_MIN_WORDS {
        return false;
    }
    if !SENTENCE_PUNCT.is_match(txt) {
        return false;
    }
    if BYLINE_RE.is_match(txt) && words < 15 {
        return false;
    }
    if DATE_RE.is_match(txt) && words < 20 {
        return false;
    }
    if link_text_length(node) > (txt.len() as f64 * 0.7) as usize {
        return false;
    }
    if is_any_tag(node, &["div"]) && !node.descendants().any(|d| is_any_tag(&d, &["p"])) {
        return false;
    }
    true
}

/// Find the start-of-prose boundary element. None when no candidate qualifies.
#[must_use]
pub fn find_content_start(root: &NodeRef, title: &str) -> Option<NodeRef> {
    let title_el = find_title_element(root, title);
    let mut started = title_el.is_none();

    for d in root.descendants() {
        if !started {
            if let Some(t) = &title_el {
                if std::ptr::eq(&*d, &**t) {
                    started = true;
                }
            }
            continue;
        }
        if is_prose_block(&d) {
            return Some(d);
        }
    }
    if title_el.is_some() {
        // Retry from start if anchored search failed.
        return find_content_start(root, "");
    }
    None
}
