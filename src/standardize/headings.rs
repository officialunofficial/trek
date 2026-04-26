//! Heading normalisation.
//!
//! * Demote every `<h1>` after the first to `<h2>` so the article body has
//!   a single H1.
//! * Drop a leading `<h2>` whose text equals the article title (the markdown
//!   layer adds the title separately).
//! * Strip trailing headings that are followed by no content.

use kuchikiki::NodeRef;

use crate::dom::walk::{
    clone_attrs, count_words, descendants_post_order, descendants_pre_order, element_children,
    is_any_tag, new_html_element, tag_name, text_content,
};
use crate::dom::{DomCtx, DomPass};

pub struct Headings;

fn rename_heading(node: &NodeRef, new_tag: &str) -> Option<NodeRef> {
    if node.as_element().is_none() {
        return None;
    }
    let attrs = clone_attrs(node);
    let new_node = new_html_element(new_tag, attrs);
    let children: Vec<NodeRef> = node.children().collect();
    for c in &children {
        new_node.append(c.clone());
    }
    node.insert_before(new_node.clone());
    node.detach();
    Some(new_node)
}

fn is_permalink_anchor(node: &NodeRef) -> bool {
    if !is_any_tag(node, &["a"]) {
        return false;
    }
    let txt = text_content(node);
    let trimmed = txt.trim();
    if matches!(trimmed, "#" | "¶" | "§" | "🔗") {
        return true;
    }
    if trimmed.is_empty() {
        return true;
    }
    false
}

impl DomPass for Headings {
    fn name(&self) -> &'static str {
        "headings"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        // Strip permalink anchors inside headings.
        for d in descendants_post_order(root) {
            if !is_any_tag(&d, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
                continue;
            }
            let kids = element_children(&d);
            for k in kids {
                if is_permalink_anchor(&k) {
                    let txt = text_content(&k);
                    if txt.trim().is_empty() {
                        k.detach();
                    }
                }
            }
        }

        // Demote excess H1s to H2 (keep the first).
        let h1s: Vec<NodeRef> = descendants_pre_order(root)
            .into_iter()
            .filter(|d| is_any_tag(d, &["h1"]))
            .collect();
        for (i, h) in h1s.iter().enumerate() {
            if i == 0 {
                continue;
            }
            rename_heading(h, "h2");
        }

        // Trailing headings: only consider top-level body children. A
        // heading nested inside a wrapper (e.g.
        // `<div class="article-header"><h1/></div>`) has no element
        // siblings — checking just `next_sibling` would mis-flag it as
        // trailing. Walk the body's children list end-to-start.
        let body = root.descendants().find(|d| is_any_tag(d, &["body"]));
        let scope = body.unwrap_or_else(|| root.clone());
        let kids = element_children(&scope);
        let mut content_seen = false;
        for k in kids.iter().rev() {
            if !is_any_tag(k, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
                let txt = text_content(k);
                if !txt.trim().is_empty() || has_replaced_content(k) {
                    content_seen = true;
                }
                continue;
            }
            if content_seen {
                break;
            }
            let txt = text_content(k);
            if count_words(&txt) <= 12 && k.parent().is_some() {
                k.detach();
            } else {
                break;
            }
        }
    }
}

fn has_replaced_content(node: &NodeRef) -> bool {
    for d in node.descendants() {
        if let Some(name) = tag_name(&d) {
            if matches!(
                name.as_str(),
                "img" | "video" | "audio" | "iframe" | "picture" | "svg" | "math" | "table"
            ) {
                return true;
            }
        }
    }
    false
}
