//! Image / figure normalizations.
//!
//! * Promote `data-src` / `data-lazy-src` / `data-original` to `src` when
//!   `src` is missing or a base64 placeholder.
//! * Flatten `<picture>` to its primary `<img>`, choosing `src` from
//!   `<source srcset>` when missing.

use kuchikiki::NodeRef;

use crate::dom::walk::{element_children, get_attr, is_any_tag};
use crate::dom::{DomCtx, DomPass};

pub struct FigureImage;

fn looks_like_base64_placeholder(src: &str) -> bool {
    if !src.starts_with("data:") {
        return false;
    }
    // Tiny base64 GIF/PNG placeholders are usually <300 chars.
    src.len() < 300
}

fn first_srcset_url(srcset: &str) -> Option<String> {
    // Take the first URL from a srcset descriptor list.
    let first = srcset.split(',').next()?.trim();
    let mut parts = first.split_whitespace();
    parts.next().map(std::string::ToString::to_string)
}

impl DomPass for FigureImage {
    fn name(&self) -> &'static str {
        "figure_image"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        // Lazy-src promotion on <img>.
        for d in root.descendants() {
            if !is_any_tag(&d, &["img"]) {
                continue;
            }
            let Some(el) = d.as_element() else { continue };
            let mut attrs = el.attributes.borrow_mut();
            let cur_src = attrs
                .get("src")
                .map(std::string::ToString::to_string)
                .unwrap_or_default();
            let needs_replace = cur_src.is_empty() || looks_like_base64_placeholder(&cur_src);
            if needs_replace {
                let alt_keys = ["data-src", "data-lazy-src", "data-original", "data-img-src"];
                for key in &alt_keys {
                    if let Some(v) = attrs.get(*key).map(std::string::ToString::to_string) {
                        if !v.is_empty() {
                            attrs.insert("src", v);
                            break;
                        }
                    }
                }
            }
            // Also promote data-srcset → srcset when missing.
            let cur_srcset = attrs
                .get("srcset")
                .map(std::string::ToString::to_string)
                .unwrap_or_default();
            if cur_srcset.is_empty() {
                if let Some(v) = attrs
                    .get("data-srcset")
                    .map(std::string::ToString::to_string)
                {
                    if !v.is_empty() {
                        attrs.insert("srcset", v);
                    }
                }
            }
        }

        // Flatten <picture>: replace with its inner <img>, hoisting srcset URL.
        let pictures: Vec<NodeRef> = root
            .descendants()
            .filter(|d| is_any_tag(d, &["picture"]))
            .collect();
        for pic in pictures {
            // Find the first <img> descendant.
            let img = pic.descendants().find(|d| is_any_tag(d, &["img"]));
            // Find a usable srcset from <source>.
            let mut chosen_src: Option<String> = None;
            for c in element_children(&pic) {
                if is_any_tag(&c, &["source"]) {
                    if let Some(ss) = get_attr(&c, "srcset") {
                        if let Some(u) = first_srcset_url(&ss) {
                            chosen_src = Some(u);
                            break;
                        }
                    }
                }
            }
            if let Some(img_node) = img {
                if let Some(img_el) = img_node.as_element() {
                    let mut attrs = img_el.attributes.borrow_mut();
                    let cur = attrs
                        .get("src")
                        .map(std::string::ToString::to_string)
                        .unwrap_or_default();
                    if (cur.is_empty() || looks_like_base64_placeholder(&cur))
                        && let Some(s) = chosen_src
                    {
                        attrs.insert("src", s);
                    }
                }
                // Move the img out and replace picture.
                pic.insert_before(img_node.clone());
                pic.detach();
            } else {
                // No img? Drop the empty picture.
                pic.detach();
            }
        }
    }
}
