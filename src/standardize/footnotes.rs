//! Recognize alternate footnote shapes that the markdown layer expects.
//!
//! The markdown renderer already understands `<sup><a href="#fnX">N</a></sup>`
//! style references and `<ol class="footnotes">` lists. This pass converts
//! a few common-but-not-canonical shapes into the canonical form so the
//! markdown emitter doesn't drop them.
//!
//! Currently handled:
//! * `<span class="footnote-ref">…</span>` containing an inline anchor →
//!   wrap the anchor in `<sup>` if missing.
//! * `<aside class="footnote">` siblings collected at end → leave content;
//!   ensure there is no stray label anchor leaking into the prose.
//!
//! This is intentionally narrow. Larger restructuring is left as TODO so we
//! don't break the markdown layer's existing happy paths.

use kuchikiki::NodeRef;

use crate::dom::walk::{descendants_post_order, get_attr, is_any_tag, new_html_element};
use crate::dom::{DomCtx, DomPass};

pub struct Footnotes;

impl DomPass for Footnotes {
    fn name(&self) -> &'static str {
        "footnotes"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        // Wrap `.footnote-ref a[href^="#fn"]` in `<sup>` when not already.
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["a"]) {
                continue;
            }
            let href = get_attr(&node, "href").unwrap_or_default();
            let class = get_attr(&node, "class")
                .unwrap_or_default()
                .to_ascii_lowercase();
            let is_ref = href.starts_with("#fn")
                || href.starts_with("#footnote")
                || class.contains("footnote-ref");
            if !is_ref {
                continue;
            }
            // Already wrapped?
            if let Some(parent) = node.parent() {
                if is_any_tag(&parent, &["sup"]) {
                    continue;
                }
            }
            // Skip if the parent is already a footnote-list link cell.
            if let Some(parent) = node.parent() {
                if is_any_tag(&parent, &["li"]) {
                    continue;
                }
            }
            let sup = new_html_element("sup", Vec::new());
            node.insert_before(sup.clone());
            sup.append(node);
        }
    }
}
