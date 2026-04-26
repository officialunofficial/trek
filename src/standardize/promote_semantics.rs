//! Promote span/div with semantic data hints to their canonical tag.
//!
//! * `<span data-as="p|h1|h2|h3|h4|h5|h6|li|blockquote">…</span>` → wrap as
//!   the named tag.
//! * `<span class="block …">` (or inline `display:block`) → `<p>`.
//! * Strip bare `<span>` elements with no attributes (deepest-first).

use kuchikiki::NodeRef;

use crate::dom::walk::{
    clone_attrs, descendants_post_order, element_children, get_attr, is_any_tag, new_html_element,
    tag_name, unwrap,
};
use crate::dom::{DomCtx, DomPass};

pub struct PromoteSemantics;

fn rename_element(node: &NodeRef, new_tag: &str) -> Option<NodeRef> {
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

impl DomPass for PromoteSemantics {
    fn name(&self) -> &'static str {
        "promote_semantics"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        // Pass 1: convert <span data-as="X"> into <X>.
        let valid_targets = ["p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "blockquote"];
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["span", "div"]) {
                continue;
            }
            if let Some(target) = get_attr(&node, "data-as") {
                if valid_targets
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(&target))
                {
                    rename_element(&node, &target.to_ascii_lowercase());
                }
            }
        }

        // Pass 2: <span class="block …"> or style="display:block" → <p>
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["span"]) {
                continue;
            }
            let mut promote = false;
            if let Some(class) = get_attr(&node, "class") {
                let class_lc = class.to_ascii_lowercase();
                if class_lc
                    .split_whitespace()
                    .any(|c| c == "block" || c == "is-block" || c == "block-text")
                {
                    promote = true;
                }
            }
            if let Some(style) = get_attr(&node, "style") {
                let style_lc = style.to_ascii_lowercase();
                if style_lc.contains("display:block") || style_lc.contains("display: block") {
                    promote = true;
                }
            }
            if promote {
                rename_element(&node, "p");
            }
        }

        // Pass 3: drop bare <span> with no attributes (deepest-first).
        // Skip spans inside <pre>/<code> — line containers may briefly
        // be attribute-less and unwrapping them collapses linebreaks.
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["span"]) {
                continue;
            }
            if node.parent().is_none() {
                continue;
            }
            let inside_code = {
                let mut cur = node.parent();
                let mut found = false;
                while let Some(p) = cur {
                    if is_any_tag(&p, &["pre", "code"]) {
                        found = true;
                        break;
                    }
                    cur = p.parent();
                }
                found
            };
            if inside_code {
                continue;
            }
            if let Some(el) = node.as_element() {
                if el.attributes.borrow().map.is_empty() {
                    unwrap(&node);
                }
            }
        }

        // Pass 4: unwrap <a> inside <code>, plus javascript: links.
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["a"]) {
                continue;
            }
            if node.parent().is_none() {
                continue;
            }
            let href = get_attr(&node, "href").unwrap_or_default();
            let inside_code = {
                let mut cur = node.parent();
                let mut found = false;
                while let Some(p) = cur {
                    if is_any_tag(&p, &["code"]) {
                        found = true;
                        break;
                    }
                    cur = p.parent();
                }
                found
            };
            if inside_code
                || href
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("javascript:")
            {
                unwrap(&node);
            }
        }

        // Pass 5: heading-wrapping anchors `<a><h2>X</h2></a>` → `<h2><a>X</a></h2>`
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["a"]) {
                continue;
            }
            let kids = element_children(&node);
            if kids.len() == 1 {
                if let Some(name) = tag_name(&kids[0]) {
                    if matches!(name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                        // Move the anchor inside the heading.
                        let h = kids[0].clone();
                        let a_attrs = clone_attrs(&node);
                        // Detach heading from anchor; replace anchor with heading;
                        // append a new <a> with original attrs containing heading children.
                        let h_children: Vec<NodeRef> = h.children().collect();
                        let new_a = new_html_element("a", a_attrs);
                        for c in &h_children {
                            new_a.append(c.clone());
                        }
                        // Empty out heading and re-append the anchor.
                        let h_clone = h.clone();
                        // Insert the heading in place of the anchor.
                        node.insert_before(h_clone.clone());
                        // Clear h's existing children (already moved into new_a).
                        for c in h_clone.children().collect::<Vec<_>>() {
                            c.detach();
                        }
                        h_clone.append(new_a);
                        node.detach();
                    }
                }
            }
        }
    }
}
