//! Shared helpers for kuchikiki-based element handlers (Track D).
//!
//! These mirror the helpers in `markdown/util.rs` but are scoped to this
//! module so we can extend them freely without touching the markdown
//! renderer.

use kuchikiki::NodeRef;
use kuchikiki::iter::NodeIterator;
use kuchikiki::traits::TendrilSink;

/// Get an attribute value from an element node.
pub fn attr(node: &NodeRef, name: &str) -> Option<String> {
    let el = node.as_element()?;
    let attrs = el.attributes.borrow();
    attrs.get(name).map(std::string::ToString::to_string)
}

/// Set an attribute on an element node. No-op for non-elements.
pub fn set_attr(node: &NodeRef, name: &str, value: &str) {
    if let Some(el) = node.as_element() {
        let mut attrs = el.attributes.borrow_mut();
        attrs.insert(name, value.to_string());
    }
}

/// Remove an attribute from an element node by local name.
pub fn remove_attr(node: &NodeRef, name: &str) {
    if let Some(el) = node.as_element() {
        let mut attrs = el.attributes.borrow_mut();
        attrs.remove(name);
    }
}

/// Lower-case tag name of an element node.
pub fn tag_name(node: &NodeRef) -> String {
    node.as_element()
        .map(|e| e.name.local.to_string().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether `node` is an element with the given (case-insensitive) tag.
pub fn is_tag(node: &NodeRef, name: &str) -> bool {
    tag_name(node).eq_ignore_ascii_case(name)
}

/// Get the class list as Vec.
pub fn class_list(node: &NodeRef) -> Vec<String> {
    attr(node, "class")
        .map(|c| c.split_whitespace().map(String::from).collect())
        .unwrap_or_default()
}

/// Whether `node` carries one of the given class tokens.
pub fn has_class(node: &NodeRef, class: &str) -> bool {
    class_list(node).iter().any(|t| t == class)
}

/// Build a fresh element with attributes by parsing a tiny HTML document
/// and walking to find the desired tag. This avoids needing direct
/// html5ever imports.
pub fn new_element(tag: &str, attrs: &[(&str, &str)]) -> NodeRef {
    let mut html = String::with_capacity(64);
    html.push_str("<!DOCTYPE html><html><body><");
    html.push_str(tag);
    for (k, v) in attrs {
        html.push(' ');
        html.push_str(k);
        html.push_str("=\"");
        html.push_str(&html_escape(v));
        html.push('"');
    }
    html.push_str("></");
    html.push_str(tag);
    html.push_str("></body></html>");
    let doc = kuchikiki::parse_html().one(html.as_str());
    for d in doc.descendants().elements() {
        if d.name.local.to_string().eq_ignore_ascii_case(tag) {
            let n = d.as_node().clone();
            n.detach();
            return n;
        }
    }
    NodeRef::new_text("")
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Move all children of `from` into `to`, preserving order.
pub fn transfer_children(from: &NodeRef, to: &NodeRef) {
    while let Some(child) = from.first_child() {
        child.detach();
        to.append(child);
    }
}

/// Collect all element descendants matching `selector` into a `Vec`.
pub fn select_all(root: &NodeRef, selector: &str) -> Vec<NodeRef> {
    match root.select(selector) {
        Ok(iter) => iter.map(|d| d.as_node().clone()).collect(),
        Err(_) => Vec::new(),
    }
}

/// First descendant matching `selector`, if any.
pub fn select_first(root: &NodeRef, selector: &str) -> Option<NodeRef> {
    root.select_first(selector)
        .ok()
        .map(|d| d.as_node().clone())
}

/// All element descendants of `node` (DFS pre-order, excludes `node`).
pub fn descendants_elements(node: &NodeRef) -> Vec<NodeRef> {
    node.descendants()
        .elements()
        .map(|d| d.as_node().clone())
        .collect()
}
