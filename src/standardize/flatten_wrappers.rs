//! Flatten purely-decorative wrapper `<div>` / `<section>` elements.
//!
//! Repeats while progress is being made. A wrapper is considered flattenable
//! when:
//! * it has no semantic role / aria-label / itemscope / data-callout
//! * its `class` attribute does not look content-bearing
//! * it has at most one element child OR all children are block-level
//!
//! Mirrors `flattenWrapperElements` from `defuddle/standardize.ts`.

use crate::dom::walk::{
    descendants_post_order, element_children, get_attr, is_any_tag, tag_name, unwrap,
};
use crate::dom::{DomCtx, DomPass};

pub struct FlattenWrappers;

const SEMANTIC_CLASS_HINTS: &[&str] = &[
    "article",
    "content",
    "footnote",
    "reference",
    "bibliography",
    "callout",
    "admonition",
    "note",
    "highlight",
    "language-",
    "math",
    "katex",
    "mathjax",
    "footnotes",
];

fn has_semantic_attrs(node: &kuchikiki::NodeRef) -> bool {
    if get_attr(node, "role").is_some()
        || get_attr(node, "aria-label").is_some()
        || get_attr(node, "itemscope").is_some()
        || get_attr(node, "data-callout").is_some()
        || get_attr(node, "data-callout-fold").is_some()
        || get_attr(node, "data-callout-metadata").is_some()
        || get_attr(node, "data-mathml").is_some()
    {
        return true;
    }
    if let Some(class) = get_attr(node, "class") {
        let lower = class.to_ascii_lowercase();
        if SEMANTIC_CLASS_HINTS.iter().any(|h| lower.contains(h)) {
            return true;
        }
    }
    false
}

fn is_unwrappable_wrapper(node: &kuchikiki::NodeRef) -> bool {
    // Only flatten generic structural wrappers — never a `body`, `html`, or
    // `figure`/`pre`/`code`/etc.
    let Some(name) = tag_name(node) else {
        return false;
    };
    if !matches!(name.as_str(), "div" | "section" | "main") {
        return false;
    }
    if has_semantic_attrs(node) {
        return false;
    }
    // Skip if it's the document body.
    if let Some(parent) = node.parent() {
        if parent.as_document().is_some() {
            return false;
        }
    } else {
        return false;
    }

    let children = element_children(node);
    if children.is_empty() {
        // Pure-text wrappers should be preserved so downstream content
        // pattern removals can target them as discrete `<div>` elements
        // (e.g. "8 min read", "By Author").
        return false;
    }

    // Heuristic: wrappers either have a single element child, or all children
    // are block-level (so flattening doesn't fuse multiple text/inline
    // streams together).
    if children.len() == 1 {
        return true;
    }

    // Don't flatten if any direct child is text/inline-only — keep paragraph
    // boundaries intact.
    let has_text = node
        .children()
        .any(|c| c.as_text().is_some_and(|t| !t.borrow().trim().is_empty()));
    if has_text {
        return false;
    }

    // All element children must be block-level.
    let block_tags = [
        "div",
        "section",
        "article",
        "aside",
        "header",
        "footer",
        "nav",
        "main",
        "p",
        "pre",
        "blockquote",
        "table",
        "ul",
        "ol",
        "dl",
        "figure",
        "form",
        "fieldset",
        "details",
        "summary",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "hr",
    ];
    children.iter().all(|c| is_any_tag(c, &block_tags))
}

impl DomPass for FlattenWrappers {
    fn name(&self) -> &'static str {
        "flatten_wrappers"
    }

    fn run(&self, root: &kuchikiki::NodeRef, _ctx: &DomCtx) {
        // Run a few iterations — flattening one wrapper can expose another.
        for _ in 0..6 {
            let mut changed = false;
            for node in descendants_post_order(root) {
                if node.parent().is_none() {
                    continue;
                }
                if is_unwrappable_wrapper(&node) {
                    unwrap(&node);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }
}
