//! Heading normalization (Track D).
//!
//! Defuddle ports `headings.ts` here. Three concrete passes:
//!
//! 1. Strip permalink anchors (`<a class="anchor" href="#…">¶</a>` and
//!    common variants).
//! 2. Strip section-number prefixes (`<span class="section-number">1.2</span>`).
//! 3. Collapse adjacent identical-text headings (e.g. duplicate `<h1>` +
//!    `<h2>` page-title pattern).

use kuchikiki::NodeRef;

use super::util::{attr, has_class, is_tag, select_all};

/// Run all heading normalization passes against `root`.
pub fn normalize_headings(root: &NodeRef) {
    strip_permalink_anchors(root);
    strip_section_numbers(root);
    collapse_adjacent_duplicates(root);
    strip_heading_buttons(root);
}

const PERMALINK_GLYPHS: &[&str] = &["#", "¶", "§", "🔗", "\u{FEFF}"];

/// True if `node` looks like a permalink anchor.
pub fn is_permalink_anchor(node: &NodeRef) -> bool {
    if !is_tag(node, "a") {
        return false;
    }
    if has_class(node, "anchor")
        || has_class(node, "permalink")
        || has_class(node, "anchor-link")
        || has_class(node, "heading-anchor")
        || has_class(node, "header-anchor")
    {
        return true;
    }
    if let Some(t) = attr(node, "title") {
        if t.to_lowercase().contains("permalink") {
            return true;
        }
    }
    if let Some(label) = attr(node, "aria-label") {
        let l = label.to_lowercase();
        if l.contains("permalink") || l.contains("anchor link") {
            return true;
        }
    }
    let text = node.text_contents();
    let trimmed = text.trim();
    if !trimmed.is_empty() && PERMALINK_GLYPHS.contains(&trimmed) {
        // Bare-glyph anchor.
        if let Some(href) = attr(node, "href") {
            if href.starts_with('#') {
                return true;
            }
        }
        return true;
    }
    false
}

fn strip_permalink_anchors(root: &NodeRef) {
    for hsel in &["h1", "h2", "h3", "h4", "h5", "h6"] {
        for h in select_all(root, hsel) {
            // Collect anchor children that look like permalinks.
            let anchors: Vec<NodeRef> = h
                .descendants()
                .filter(|n| is_tag(n, "a") && is_permalink_anchor(n))
                .collect();
            for a in anchors {
                a.detach();
            }
        }
    }
}

fn strip_section_numbers(root: &NodeRef) {
    for hsel in &["h1", "h2", "h3", "h4", "h5", "h6"] {
        for h in select_all(root, hsel) {
            let nums: Vec<NodeRef> = h
                .descendants()
                .filter(|n| has_class(n, "section-number") || has_class(n, "header-section-number"))
                .collect();
            for n in nums {
                n.detach();
            }
        }
    }
}

fn strip_heading_buttons(root: &NodeRef) {
    for hsel in &["h1", "h2", "h3", "h4", "h5", "h6"] {
        for h in select_all(root, hsel) {
            let buttons: Vec<NodeRef> = h.descendants().filter(|n| is_tag(n, "button")).collect();
            for b in buttons {
                b.detach();
            }
        }
    }
}

fn collapse_adjacent_duplicates(root: &NodeRef) {
    // For each heading node, check if its immediate next-element-sibling is
    // a heading with identical normalized text. If so, drop the later one.
    let headings = select_all(root, "h1, h2, h3, h4, h5, h6");
    let mut to_drop: Vec<NodeRef> = Vec::new();
    for h in headings {
        let mut sib = h.next_sibling();
        while let Some(s) = sib.clone() {
            if s.as_element().is_some() {
                break;
            }
            sib = s.next_sibling();
        }
        let Some(next) = sib else { continue };
        if !next
            .as_element()
            .map(|e| {
                let n = e.name.local.to_string().to_ascii_lowercase();
                matches!(n.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
            })
            .unwrap_or(false)
        {
            continue;
        }
        let a = norm(&h.text_contents());
        let b = norm(&next.text_contents());
        if !a.is_empty() && a == b {
            to_drop.push(next);
        }
    }
    for d in to_drop {
        d.detach();
    }
}

fn norm(s: &str) -> String {
    s.replace('\u{00A0}', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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
    fn permalink_anchor_is_stripped() {
        let html = r##"<html><body><h2 id="x">Title <a class="anchor" href="#x">#</a></h2></body></html>"##;
        let root = parse(html);
        normalize_headings(&root);
        let out = serialize(&root);
        assert!(!out.contains("anchor"), "got: {out}");
        assert!(out.contains("Title"), "got: {out}");
    }

    #[test]
    fn section_number_is_stripped() {
        let html =
            r#"<html><body><h2><span class="section-number">1.2</span> Heading</h2></body></html>"#;
        let root = parse(html);
        normalize_headings(&root);
        let out = serialize(&root);
        assert!(!out.contains("1.2"), "got: {out}");
        assert!(out.contains("Heading"), "got: {out}");
    }

    #[test]
    fn adjacent_duplicates_collapse() {
        let html = r#"<html><body><h1>Hello</h1><h1>Hello</h1><p>x</p></body></html>"#;
        let root = parse(html);
        normalize_headings(&root);
        let out = serialize(&root);
        assert_eq!(out.matches("Hello").count(), 1, "got: {out}");
    }
}
