//! Tree-walking helpers for `kuchikiki` `NodeRef`s.
//!
//! These primitives are used across the standardize/removal passes; they
//! intentionally have no public API guarantees other than what the modules
//! that consume them rely on.

use html5ever::{LocalName, QualName, namespace_url, ns};
use kuchikiki::{Attribute, ExpandedName, NodeRef};

/// Build a list of (`ExpandedName`, `Attribute`) suitable for
/// `NodeRef::new_element` from a slice of (name, value) pairs.
#[must_use]
pub fn build_attrs(pairs: &[(&str, &str)]) -> Vec<(ExpandedName, Attribute)> {
    pairs
        .iter()
        .map(|(k, v)| {
            (
                ExpandedName::new(ns!(), LocalName::from(*k)),
                Attribute {
                    prefix: None,
                    value: (*v).to_string(),
                },
            )
        })
        .collect()
}

/// Clone the existing attributes off `node` into the form `new_element` expects.
#[must_use]
pub fn clone_attrs(node: &NodeRef) -> Vec<(ExpandedName, Attribute)> {
    let Some(el) = node.as_element() else {
        return Vec::new();
    };
    el.attributes
        .borrow()
        .map
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Construct a fresh element node (HTML namespace) with the given tag name
/// and attribute list.
#[must_use]
pub fn new_html_element(tag: &str, attrs: Vec<(ExpandedName, Attribute)>) -> NodeRef {
    let qname = QualName::new(None, ns!(html), LocalName::from(tag));
    NodeRef::new_element(qname, attrs)
}

/// Return the lowercase tag name of `node`, if it is an element.
#[must_use]
pub fn tag_name(node: &NodeRef) -> Option<String> {
    node.as_element().map(|el| el.name.local.to_string())
}

/// True when `node` is an element with one of the given tag names.
#[must_use]
pub fn is_any_tag(node: &NodeRef, names: &[&str]) -> bool {
    if let Some(el) = node.as_element() {
        let local: &str = &el.name.local;
        return names.iter().any(|n| n.eq_ignore_ascii_case(local));
    }
    false
}

/// Return an attribute value if present.
#[must_use]
pub fn get_attr(node: &NodeRef, name: &str) -> Option<String> {
    let el = node.as_element()?;
    let attrs = el.attributes.borrow();
    attrs.get(name).map(std::string::ToString::to_string)
}

/// Concatenate descendant text content.
#[must_use]
pub fn text_content(node: &NodeRef) -> String {
    let mut buf = String::new();
    for d in node.descendants() {
        if let Some(t) = d.as_text() {
            buf.push_str(&t.borrow());
        }
    }
    buf
}

/// Word count over visible text content (ASCII whitespace split).
#[must_use]
pub fn count_words(s: &str) -> usize {
    s.split_whitespace().filter(|w| !w.is_empty()).count()
}

/// Combined link-text length within a subtree.
#[must_use]
pub fn link_text_length(node: &NodeRef) -> usize {
    let mut total = 0usize;
    for d in node.descendants() {
        if is_any_tag(&d, &["a"]) {
            total += text_content(&d).len();
        }
    }
    total
}

/// Iterate over an element's element-only children (skipping text/comment).
pub fn element_children(node: &NodeRef) -> Vec<NodeRef> {
    node.children()
        .filter(|c| c.as_element().is_some())
        .collect()
}

/// All descendants, deepest-first (post-order).
pub fn descendants_post_order(root: &NodeRef) -> Vec<NodeRef> {
    let mut out = Vec::new();
    fn visit(n: &NodeRef, out: &mut Vec<NodeRef>) {
        for c in n.children() {
            visit(&c, out);
        }
        out.push(n.clone());
    }
    for c in root.children() {
        visit(&c, &mut out);
    }
    out
}

/// Document-order pre-order list of all descendants.
pub fn descendants_pre_order(root: &NodeRef) -> Vec<NodeRef> {
    root.descendants().collect()
}

/// Detach a node from its parent without panicking if already detached.
pub fn detach(node: &NodeRef) {
    node.detach();
}

/// True when `ancestor` strictly contains `el`.
#[must_use]
pub fn contains(ancestor: &NodeRef, el: &NodeRef) -> bool {
    let mut cur = el.parent();
    while let Some(p) = cur {
        if std::ptr::eq(&*p, &**ancestor) {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Walk up parent chain looking for an element with the given tag name (case-insensitive).
#[must_use]
pub fn closest_tag(node: &NodeRef, names: &[&str]) -> Option<NodeRef> {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if is_any_tag(&p, names) {
            return Some(p);
        }
        cur = p.parent();
    }
    None
}

/// True when this element or any ancestor matches one of the given tag names.
#[must_use]
pub fn is_or_inside(node: &NodeRef, names: &[&str]) -> bool {
    if is_any_tag(node, names) {
        return true;
    }
    closest_tag(node, names).is_some()
}

/// Replace `node` with its children.
pub fn unwrap(node: &NodeRef) {
    // Move all children to be siblings before `node`, then detach node.
    let children: Vec<NodeRef> = node.children().collect();
    for c in &children {
        node.insert_before(c.clone());
    }
    node.detach();
}

/// True when an element looks "empty" — no non-whitespace text and no
/// element children that are themselves non-empty content.
#[must_use]
pub fn is_visually_empty(node: &NodeRef) -> bool {
    if !node.as_element().is_some() {
        return false;
    }
    for d in node.descendants() {
        if let Some(t) = d.as_text() {
            let s = t.borrow();
            if !s.trim().is_empty() && s.trim() != "\u{00A0}" {
                return false;
            }
        }
        if let Some(el) = d.as_element() {
            let local: &str = &el.name.local;
            // Self-closing/replaced elements that count as content even when empty.
            if matches!(
                local,
                "img"
                    | "video"
                    | "audio"
                    | "iframe"
                    | "picture"
                    | "source"
                    | "svg"
                    | "math"
                    | "input"
                    | "br"
                    | "hr"
                    | "embed"
                    | "object"
                    | "canvas"
            ) {
                return false;
            }
        }
    }
    true
}
