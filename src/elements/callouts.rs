//! Callout normalization (Track D).
//!
//! Defuddle ports `callouts.ts` here. Five sources are detected and
//! rewritten to a single canonical shape that the markdown renderer
//! already understands (`src/markdown/mod.rs::render_callout`):
//!
//! ```html
//! <div data-callout="warning" class="callout">
//!   <div class="callout-title">
//!     <div class="callout-title-inner">Warning</div>
//!   </div>
//!   <div class="callout-content"><!-- transferred body --></div>
//! </div>
//! ```
//!
//! Sources handled:
//! 1. Obsidian Publish callouts (`.callout.is-collapsed` / `.callout.is-collapsible`).
//! 2. GitHub markdown alerts (`.markdown-alert.markdown-alert-{type}` or a
//!    `<blockquote>` whose first line is `[!NOTE]`/`[!WARNING]` etc.).
//! 3. Aside callouts (`<aside class="callout-foo">`).
//! 4. Hugo / Docsy admonitions (`.admonition.note` etc.).
//! 5. Bootstrap alerts (`.alert.alert-info` etc.).
//!
//! Runs *early* so the selector-removal step doesn't strip `.alert` or
//! `.admonition`.

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use super::util::{
    attr, class_list, descendants_elements, has_class, is_tag, new_element, remove_attr,
    select_all, select_first, set_attr, transfer_children,
};

/// Normalize callouts in `root` (in place).
pub fn normalize_callouts(root: &NodeRef) {
    process_obsidian_collapsed(root);
    process_github_alerts(root);
    process_github_blockquote_alerts(root);
    process_aside_callouts(root);
    process_admonitions(root);
    process_bootstrap_alerts(root);
}

/// Capitalize the first ASCII letter (UTF-8 safe).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Build the canonical callout shape from a body source. Children of
/// `body_source` are detached and moved into the new `.callout-content`.
fn build_callout(callout_type: &str, title: &str, body_source: &NodeRef) -> NodeRef {
    let outer = new_element(
        "div",
        &[("data-callout", callout_type), ("class", "callout")],
    );

    // Title
    let title_div = new_element("div", &[("class", "callout-title")]);
    let title_inner = new_element("div", &[("class", "callout-title-inner")]);
    title_inner.append(NodeRef::new_text(title));
    title_div.append(title_inner);
    outer.append(title_div);

    // Content
    let content_div = new_element("div", &[("class", "callout-content")]);
    transfer_children(body_source, &content_div);
    outer.append(content_div);

    outer
}

/// Replace `old` with `new` in the parent's child list.
fn replace_with(old: &NodeRef, new: NodeRef) {
    old.insert_before(new);
    old.detach();
}

// ---------------------------------------------------------------------------
// 1. Obsidian Publish: unwrap is-collapsed / is-collapsible markers
// ---------------------------------------------------------------------------

fn process_obsidian_collapsed(root: &NodeRef) {
    let nodes = select_all(root, ".callout.is-collapsed, .callout.is-collapsible");
    for el in nodes {
        let collapsed = has_class(&el, "is-collapsed");

        // Remove is-collapsed and is-collapsible from class list
        let new_classes: Vec<String> = class_list(&el)
            .into_iter()
            .filter(|c| c != "is-collapsed" && c != "is-collapsible")
            .collect();
        if new_classes.is_empty() {
            remove_attr(&el, "class");
        } else {
            set_attr(&el, "class", &new_classes.join(" "));
        }

        // Preserve fold state via data-callout-fold.
        if attr(&el, "data-callout-fold").is_none() {
            set_attr(&el, "data-callout-fold", if collapsed { "-" } else { "+" });
        }

        // Strip .callout-fold helper.
        if let Some(fold) = select_first(&el, ".callout-fold") {
            fold.detach();
        }

        // Drop inline `display: none` from .callout-content (otherwise the
        // hidden-element pass strips the body).
        if let Some(content) = select_first(&el, ".callout-content") {
            if let Some(style) = attr(&content, "style") {
                static DISPLAY_NONE: Lazy<Regex> = Lazy::new(|| {
                    Regex::new(r"(?i)display\s*:\s*none\s*;?").expect("display:none regex")
                });
                let cleaned = DISPLAY_NONE.replace_all(&style, "").trim().to_string();
                if cleaned.is_empty() {
                    remove_attr(&content, "style");
                } else {
                    set_attr(&content, "style", &cleaned);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2a. GitHub markdown alerts (div.markdown-alert.markdown-alert-{type})
// ---------------------------------------------------------------------------

fn process_github_alerts(root: &NodeRef) {
    let nodes = select_all(root, ".markdown-alert");
    for el in nodes {
        let cls = class_list(&el);
        let type_class = cls
            .iter()
            .find(|c| c.starts_with("markdown-alert-") && c.as_str() != "markdown-alert");
        let callout_type = type_class
            .map(|c| c.trim_start_matches("markdown-alert-").to_lowercase())
            .unwrap_or_else(|| "note".to_string());
        let title = capitalize(&callout_type);

        // Drop the icon/title element.
        if let Some(t) = select_first(&el, ".markdown-alert-title") {
            t.detach();
        }

        let new_node = build_callout(&callout_type, &title, &el);
        replace_with(&el, new_node);
    }
}

// ---------------------------------------------------------------------------
// 2b. GitHub blockquote alerts: <blockquote> whose first line is `[!NOTE]`
// ---------------------------------------------------------------------------

static GH_BLOCKQUOTE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*\[!\s*(NOTE|TIP|IMPORTANT|WARNING|CAUTION|DANGER)\s*\]\s*(.*)$")
        .expect("github blockquote regex")
});

fn process_github_blockquote_alerts(root: &NodeRef) {
    // Find blockquotes whose first non-empty descendant text starts with
    // `[!TYPE]`.
    let blockquotes = select_all(root, "blockquote");
    for bq in blockquotes {
        // Skip already-converted callouts.
        if attr(&bq, "data-callout").is_some() {
            continue;
        }
        // Look at first text descendant.
        let text = bq.text_contents();
        let first_line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let Some(caps) = GH_BLOCKQUOTE_RE.captures(first_line) else {
            continue;
        };
        let kind = caps
            .get(1)
            .map(|m| m.as_str().to_lowercase())
            .unwrap_or_default();
        let title = capitalize(&kind);

        // Strip the `[!TYPE]` token from the first text node we encounter.
        strip_alert_marker(&bq);

        let new_node = build_callout(&kind, &title, &bq);
        replace_with(&bq, new_node);
    }
}

/// Walk a subtree and remove the `[!TYPE]` marker from the first non-empty
/// text node.
fn strip_alert_marker(root: &NodeRef) {
    for node in root.descendants() {
        let Some(t) = node.as_text() else { continue };
        let raw = t.borrow().clone();
        let trimmed = raw.trim_start();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(caps) = GH_BLOCKQUOTE_RE.captures(trimmed) {
            // Replace whole text node with the captured tail.
            let tail = caps.get(2).map(|m| m.as_str()).unwrap_or("").trim_start();
            let mut new_val = String::new();
            // Preserve leading whitespace of the original.
            let leading: String = raw.chars().take_while(|c| c.is_whitespace()).collect();
            new_val.push_str(&leading);
            new_val.push_str(tail);
            *t.borrow_mut() = new_val;
            return;
        }
        // First non-empty text node didn't match — bail; the marker isn't
        // in a leading text node.
        return;
    }
}

// ---------------------------------------------------------------------------
// 3. Aside callouts: <aside class="callout-*">
// ---------------------------------------------------------------------------

fn process_aside_callouts(root: &NodeRef) {
    let nodes = select_all(root, "aside[class*='callout']");
    for el in nodes {
        if !is_tag(&el, "aside") {
            continue;
        }
        // Skip if already callout-shaped.
        if attr(&el, "data-callout").is_some() {
            continue;
        }
        let cls = class_list(&el);
        let type_class = cls.iter().find(|c| c.starts_with("callout-"));
        let Some(type_class) = type_class else {
            continue;
        };
        let kind = type_class.trim_start_matches("callout-").to_lowercase();
        let title = capitalize(&kind);
        // Body source is `.callout-content` if present, else the element itself.
        let body_source = select_first(&el, ".callout-content").unwrap_or_else(|| el.clone());
        let new_node = build_callout(&kind, &title, &body_source);
        replace_with(&el, new_node);
    }
}

// ---------------------------------------------------------------------------
// 4. Hugo / Docsy admonitions: .admonition with type class
// ---------------------------------------------------------------------------

const ADMONITION_TYPES: &[&str] = &[
    "info",
    "warning",
    "note",
    "tip",
    "danger",
    "caution",
    "important",
    "abstract",
    "success",
    "question",
    "failure",
    "bug",
    "example",
    "quote",
];

fn process_admonitions(root: &NodeRef) {
    let nodes = select_all(root, ".admonition");
    for el in nodes {
        if attr(&el, "data-callout").is_some() {
            continue;
        }
        let cls = class_list(&el);
        // Look for a class that is either a literal admonition type, or
        // an `admonition-{type}` form (Hugo Docsy).
        let mut kind: Option<String> = None;
        for c in &cls {
            if ADMONITION_TYPES.contains(&c.as_str()) {
                kind = Some(c.clone());
                break;
            }
            if let Some(suffix) = c.strip_prefix("admonition-") {
                if ADMONITION_TYPES.contains(&suffix) {
                    kind = Some(suffix.to_string());
                    break;
                }
            }
        }
        let kind = kind.unwrap_or_else(|| "note".to_string());

        // Title from .admonition-title.
        let title_el = select_first(&el, ".admonition-title");
        let title_text = title_el
            .as_ref()
            .map(|t| t.text_contents().trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| capitalize(&kind));
        if let Some(t) = title_el {
            t.detach();
        }

        let body_source = select_first(&el, ".admonition-content")
            .or_else(|| select_first(&el, ".details-content"))
            .unwrap_or_else(|| el.clone());

        let new_node = build_callout(&kind, &title_text, &body_source);
        replace_with(&el, new_node);
    }
}

// ---------------------------------------------------------------------------
// 5. Bootstrap alerts: .alert.alert-{type} (skip alert-dismissible)
// ---------------------------------------------------------------------------

fn process_bootstrap_alerts(root: &NodeRef) {
    let nodes = select_all(root, ".alert");
    for el in nodes {
        if attr(&el, "data-callout").is_some() {
            continue;
        }
        let cls = class_list(&el);
        if !cls.iter().any(|c| c == "alert") {
            continue;
        }
        let type_class = cls
            .iter()
            .find(|c| c.starts_with("alert-") && c.as_str() != "alert-dismissible");
        let Some(type_class) = type_class else {
            continue;
        };
        let kind = type_class.trim_start_matches("alert-").to_lowercase();

        let title_el =
            select_first(&el, ".alert-heading").or_else(|| select_first(&el, ".alert-title"));
        let title_text = title_el
            .as_ref()
            .map(|t| t.text_contents().trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| capitalize(&kind));
        if let Some(t) = title_el {
            t.detach();
        }

        let new_node = build_callout(&kind, &title_text, &el);
        replace_with(&el, new_node);
    }
}

// Suppress unused-import warning when no descendants_elements call survives a refactor.
#[allow(dead_code)]
fn _keep_imports(_n: &NodeRef) {
    let _ = descendants_elements;
}

#[cfg(test)]
mod tests {
    use super::*;
    use kuchikiki::traits::TendrilSink;

    fn parse(html: &str) -> NodeRef {
        kuchikiki::parse_html().one(html)
    }

    fn serialize(node: &NodeRef) -> String {
        let mut buf = Vec::new();
        node.serialize(&mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn github_alert_blockquote_is_normalized() {
        let html =
            r#"<html><body><blockquote><p>[!WARNING] heads up</p></blockquote></body></html>"#;
        let root = parse(html);
        normalize_callouts(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"data-callout="warning""#), "got: {out}");
        assert!(out.contains("callout-title-inner"), "got: {out}");
        assert!(out.contains("Warning"), "got: {out}");
        assert!(out.contains("heads up"), "got: {out}");
    }

    #[test]
    fn github_markdown_alert_div_is_normalized() {
        let html = r#"<html><body><div class="markdown-alert markdown-alert-tip"><p class="markdown-alert-title">Tip</p><p>be careful</p></div></body></html>"#;
        let root = parse(html);
        normalize_callouts(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"data-callout="tip""#), "got: {out}");
        assert!(out.contains("Tip"));
        assert!(out.contains("be careful"));
    }

    #[test]
    fn admonition_is_normalized() {
        let html = r#"<html><body><div class="admonition note"><p class="admonition-title">My note</p><p>body</p></div></body></html>"#;
        let root = parse(html);
        normalize_callouts(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"data-callout="note""#), "got: {out}");
        assert!(out.contains("My note"));
        assert!(out.contains("body"));
    }

    #[test]
    fn bootstrap_alert_is_normalized() {
        let html = r#"<html><body><div class="alert alert-info"><h4 class="alert-heading">Howdy</h4><p>info body</p></div></body></html>"#;
        let root = parse(html);
        normalize_callouts(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"data-callout="info""#), "got: {out}");
        assert!(out.contains("Howdy"));
        assert!(out.contains("info body"));
    }

    #[test]
    fn aside_callout_is_normalized() {
        let html = r#"<html><body><aside class="callout-warning"><div class="callout-content"><p>watch out</p></div></aside></body></html>"#;
        let root = parse(html);
        normalize_callouts(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"data-callout="warning""#), "got: {out}");
        assert!(out.contains("watch out"));
    }

    #[test]
    fn obsidian_collapsed_is_unwrapped() {
        let html = r#"<html><body><div class="callout is-collapsed" data-callout="info"><div class="callout-title"><div class="callout-title-inner">Info</div></div><div class="callout-content" style="display:none">body</div></div></body></html>"#;
        let root = parse(html);
        normalize_callouts(&root);
        let out = serialize(&root);
        assert!(!out.contains("is-collapsed"), "got: {out}");
        assert!(out.contains(r#"data-callout-fold="-""#), "got: {out}");
        assert!(!out.contains("display:none"), "got: {out}");
    }
}
