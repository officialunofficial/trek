//! Remove elements hidden via inline `style` or hidden CSS class names.

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::dom::walk::{get_attr, is_any_tag};
use crate::dom::{DomCtx, DomPass};

pub struct Hidden;

static HIDDEN_STYLE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:^|;\s*)(?:display\s*:\s*none|visibility\s*:\s*hidden|opacity\s*:\s*0)(?:\s*;|\s*$)")
        .expect("valid regex")
});

fn contains_math(node: &NodeRef) -> bool {
    for d in node.descendants() {
        if is_any_tag(&d, &["math"]) {
            return true;
        }
        if let Some(class) = get_attr(&d, "class") {
            let lc = class.to_ascii_lowercase();
            if lc.contains("katex-mathml") || lc.contains("mathml") {
                return true;
            }
        }
        if get_attr(&d, "data-mathml").is_some() {
            return true;
        }
    }
    false
}

fn class_marks_hidden(class: &str) -> bool {
    for tok in class.split_whitespace() {
        // Tailwind arbitrary variants `[&_.x]:hidden`, `[&[data-state=open]]:hidden`,
        // `group-hover/foo:hidden`, etc. are conditional, not unconditional.
        if tok.contains('[') || tok.contains(']') {
            continue;
        }
        if tok == "hidden" || tok == "invisible" {
            return true;
        }
        if tok.ends_with(":hidden") || tok.ends_with(":invisible") {
            // Responsive prefixes (`md:`, `sm:`, `lg:`) are responsive and may
            // be unhidden at other breakpoints; treat as hidden if no
            // companion `md:flex` etc. is present (the caller checks
            // `has_responsive_show`).
            return true;
        }
    }
    false
}

fn has_responsive_show(class: &str) -> bool {
    class.split_whitespace().any(|t| {
        t.contains(':')
            && (t.ends_with(":flex")
                || t.ends_with(":block")
                || t.ends_with(":inline")
                || t.ends_with(":grid"))
    })
}

impl DomPass for Hidden {
    fn name(&self) -> &'static str {
        "hidden"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        let mut to_remove: Vec<NodeRef> = Vec::new();
        for d in root.descendants() {
            let Some(_el) = d.as_element() else { continue };
            if is_any_tag(&d, &["html", "body", "head"]) {
                continue;
            }
            // Preserve elements containing math.
            if contains_math(&d) {
                continue;
            }
            // Inline style.
            if let Some(style) = get_attr(&d, "style") {
                if HIDDEN_STYLE_RE.is_match(&style) {
                    to_remove.push(d.clone());
                    continue;
                }
            }
            // Defuddle does NOT remove elements based on the `hidden`
            // attribute or `aria-hidden=true` on its own — many SSR
            // streaming patterns (React streaming, AMP, etc.) place
            // content inside `<div hidden>` and rely on JS to unhide it.
            // We intentionally do nothing for those signals.
            // Class-based.
            if let Some(class) = get_attr(&d, "class") {
                if has_responsive_show(&class) {
                    continue;
                }
                if class_marks_hidden(&class) {
                    to_remove.push(d.clone());
                    continue;
                }
            }
        }
        for n in to_remove {
            if n.parent().is_some() {
                n.detach();
            }
        }
    }
}
