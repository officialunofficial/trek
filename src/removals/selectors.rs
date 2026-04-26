//! Apply Trek's selector lists (constants.rs) as real removal rules.
//!
//! The lol_html-based pre-pass already does coarse class-name removal.
//! This pass picks up the long tail: `id`, `data-testid`, `data-component`
//! etc. that lol_html cannot easily target, plus partial-attribute matches
//! against `PARTIAL_SELECTORS`.

use kuchikiki::NodeRef;

use crate::constants::{PARTIAL_SELECTORS, TEST_ATTRIBUTES};
use crate::dom::walk::{closest_tag, is_any_tag};
use crate::dom::{DomCtx, DomPass};

pub struct Selectors;

const FOOTNOTE_HOST_TAGS: &[&str] = &[];

fn matches_partial_selector(value: &str) -> Option<&'static str> {
    let v = value.to_ascii_lowercase();
    for pat in PARTIAL_SELECTORS {
        if v.contains(pat) {
            return Some(*pat);
        }
    }
    None
}

fn is_inside_pre_or_code(node: &NodeRef) -> bool {
    closest_tag(node, &["pre", "code"]).is_some() || is_any_tag(node, &["pre", "code"])
}

fn is_heading(node: &NodeRef) -> bool {
    is_any_tag(node, &["h1", "h2", "h3", "h4", "h5", "h6"])
}

fn _appears_to_be_footnote(node: &NodeRef) -> bool {
    // Conservative: only protect explicit footnote shapes.
    use crate::dom::walk::get_attr;
    if let Some(class) = get_attr(node, "class") {
        let lc = class.to_ascii_lowercase();
        if lc.contains("footnote") || lc.contains("references") || lc.contains("citation") {
            return true;
        }
    }
    if let Some(id) = get_attr(node, "id") {
        let lc = id.to_ascii_lowercase();
        if lc.starts_with("fn") || lc.starts_with("fnref") || lc.contains("footnote") {
            return true;
        }
    }
    false
}

fn class_token_matches_partial(value: &str, attr: &str) -> bool {
    // `hidden` etc. are covered by exact selectors. For partial matching we
    // honor responsive classes — `hidden sm:flex` should NOT be removed.
    if attr == "class" {
        let tokens: Vec<&str> = value.split_whitespace().collect();
        // If token list contains a "show" pseudo (e.g. `sm:flex`) skip
        // matching the bare `hidden`/`invisible`.
        let has_responsive_show = tokens
            .iter()
            .any(|t| t.contains(':') && (t.ends_with(":flex") || t.ends_with(":block") || t.ends_with(":inline")));
        // Skip Tailwind arbitrary variants (`[&_.foo]:hidden`) — those are
        // descendant-conditional and never an indication that *this*
        // element is chrome.
        let kept: Vec<&str> = tokens
            .iter()
            .filter(|t| !t.contains('[') && !t.contains(']'))
            .filter(|t| {
                if !has_responsive_show {
                    return true;
                }
                **t != "hidden" && **t != "invisible"
            })
            .copied()
            .collect();
        for tok in &kept {
            if matches_partial_selector(tok).is_some() {
                return true;
            }
        }
        return false;
    }
    matches_partial_selector(value).is_some()
}

impl DomPass for Selectors {
    fn name(&self) -> &'static str {
        "selectors"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        let _ = FOOTNOTE_HOST_TAGS;
        // Collect candidates, then remove. (Mutating during iter is risky.)
        let mut to_remove: Vec<NodeRef> = Vec::new();
        for d in root.descendants() {
            let Some(el) = d.as_element() else { continue };
            // Skip elements inside pre/code (syntax-highlighting spans look
            // like "promo" classes).
            if is_inside_pre_or_code(&d) {
                continue;
            }
            // For headings, only check class — IDs are auto-slugs and
            // data-testid (e.g. "article-header") cause false positives.
            let is_h = is_heading(&d);
            let mut matched = false;
            for attr in TEST_ATTRIBUTES {
                if is_h && *attr != "class" {
                    continue;
                }
                let attrs = el.attributes.borrow();
                if let Some(value) = attrs.get(*attr).map(std::string::ToString::to_string) {
                    if class_token_matches_partial(&value, attr) {
                        matched = true;
                        break;
                    }
                }
            }
            if matched {
                // Don't disconnect ancestors of <body>.
                if is_any_tag(&d, &["html", "body"]) {
                    continue;
                }
                to_remove.push(d.clone());
            }
        }
        for n in to_remove {
            // Skip if already detached, or if it's an anchor inside heading
            // (heading transform handles those).
            if n.parent().is_none() {
                continue;
            }
            if is_any_tag(&n, &["a"])
                && closest_tag(&n, &["h1", "h2", "h3", "h4", "h5", "h6"]).is_some()
            {
                continue;
            }
            n.detach();
        }
    }
}
