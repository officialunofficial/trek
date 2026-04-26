//! Link / anchor handling for the Markdown converter.

use kuchikiki::NodeRef;

use super::util::attr;

/// Pick the best `href` for an `<a>` element. Returns `None` if the link
/// should be dropped entirely (e.g. footnote backrefs, javascript: links).
pub fn link_href(node: &NodeRef) -> Option<String> {
    let href = attr(node, "href")?;
    if href.is_empty() {
        return None;
    }
    let trimmed = href.trim();
    if trimmed.eq_ignore_ascii_case("javascript:void(0)") || trimmed.starts_with("javascript:") {
        return None;
    }
    Some(normalize_url(trimmed))
}

/// Normalize a URL by appending a trailing slash to bare-host URLs. Defuddle
/// does this implicitly via `new URL().toString()`, which always adds a
/// trailing slash to scheme+host strings.
fn normalize_url(href: &str) -> String {
    // Only act on absolute http(s) URLs.
    let lower = href.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return href.to_string();
    }
    // Find the path portion (after the first `/` past scheme://).
    let scheme_end = href.find("://").map(|i| i + 3).unwrap_or(0);
    let after_scheme = &href[scheme_end..];
    // If there's no `/` after the host, add one. We must avoid clobbering
    // paths/queries/fragments.
    if after_scheme.contains('/') || after_scheme.contains('?') || after_scheme.contains('#') {
        return href.to_string();
    }
    format!("{href}/")
}

/// True if this anchor is a footnote backref (link from footnote definition
/// back to the in-text reference); these should not appear in markdown.
pub fn is_backref(node: &NodeRef) -> bool {
    if let Some(href) = attr(node, "href") {
        if href.contains("#fnref") || href.contains("#cite_ref") {
            return true;
        }
    }
    if let Some(class) = attr(node, "class") {
        if class.contains("footnote-backref") || class.contains("backref") {
            return true;
        }
    }
    if let Some(rel) = attr(node, "rel") {
        if rel.split_whitespace().any(|t| t == "footnote-back") {
            return true;
        }
    }
    false
}

/// Whether the anchor target points at a footnote definition (in-text ref).
/// If so we want to emit `[^N]` rather than a normal link.
pub fn footnote_ref_id(node: &NodeRef) -> Option<String> {
    let href = attr(node, "href")?;
    // Common patterns: #fn:1, #fn1, #footnote-1, #cite_note-name-1
    let id = href.strip_prefix('#')?;
    if let Some(rest) = id.strip_prefix("fn:") {
        return Some(normalize_fn_id(rest));
    }
    if let Some(rest) = id.strip_prefix("fn-") {
        return Some(normalize_fn_id(rest));
    }
    if let Some(rest) = id.strip_prefix("fn") {
        if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            return Some(normalize_fn_id(rest));
        }
    }
    if let Some(rest) = id.strip_prefix("footnote-") {
        return Some(normalize_fn_id(rest));
    }
    if let Some(rest) = id.strip_prefix("cite_note-") {
        return Some(normalize_fn_id(rest));
    }
    // Pulldown-cmark style: `<a href="#1">1</a>` inside a <sup> — the
    // target is just a digit. We accept this if the link's text matches
    // the id (so we don't capture every numeric anchor).
    if id.chars().all(|c| c.is_ascii_digit()) {
        let text = node.text_contents();
        let trimmed = text.trim();
        if trimmed == id {
            return Some(id.to_string());
        }
    }
    None
}

/// Normalize a footnote id: drop everything from the first `-` onwards
/// (Defuddle compat — `fnref:1-2` → `1`).
fn normalize_fn_id(raw: &str) -> String {
    raw.split('-').next().unwrap_or(raw).to_string()
}
