//! `<figure>`, `<picture>`, `srcset` handling.

use kuchikiki::NodeRef;

use super::util::{attr, is_tag};

/// Pick the best image URL from an `<img>` node, considering `srcset`.
///
/// Defuddle's `getBestImageSrc` (markdown.ts:41–77) picks the highest-`Nw`
/// candidate, falling back to plain `src`. CDN URLs frequently embed commas
/// in their query strings, so we tokenize on **whitespace runs** and look
/// for the trailing `Nw` width descriptor on each fragment.
pub fn best_img_src(img: &NodeRef) -> Option<String> {
    // First check the parent <picture> for <source srcset>.
    if let Some(parent) = img.parent() {
        if super::util::is_tag(&parent, "picture") {
            for src_node in parent.children() {
                if super::util::is_tag(&src_node, "source") {
                    if let Some(s) = attr(&src_node, "srcset").or_else(|| attr(&src_node, "srcSet"))
                    {
                        if let Some(best) = pick_from_srcset(&s) {
                            return Some(best);
                        }
                    }
                }
            }
        }
    }
    if let Some(srcset) = attr(img, "srcset").or_else(|| attr(img, "srcSet")) {
        if let Some(best) = pick_from_srcset(&srcset) {
            // Avoid picking placeholder data URIs.
            if !best.starts_with("data:") {
                return Some(best);
            }
        }
    }
    let src_attrs = ["src", "data-src", "data-original", "data-lazy-src"];
    for a in src_attrs {
        if let Some(s) = attr(img, a) {
            if !s.is_empty() && !s.starts_with("data:") {
                return Some(s);
            }
        }
    }
    // As a last resort, accept a data: src.
    if let Some(s) = attr(img, "src") {
        if !s.is_empty() {
            return Some(s);
        }
    }
    None
}

fn pick_from_srcset(srcset: &str) -> Option<String> {
    // Split into fragments. Defuddle uses commas, but CDN URLs may contain
    // commas inside query strings, so we use the presence of an `Nw` or `Nx`
    // descriptor to validate fragments. Tokenize on whitespace first, then
    // pair URL with descriptor.
    let tokens: Vec<&str> = srcset.split_whitespace().collect();
    let mut best_w: Option<u32> = None;
    let mut best_url: Option<String> = None;

    let mut i = 0;
    while i < tokens.len() {
        let tok = tokens[i];
        // A descriptor token ends in `w` or `x` and the rest parses as int/float.
        let is_desc = (tok.ends_with('w') || tok.ends_with('x'))
            && tok[..tok.len() - 1]
                .chars()
                .all(|c| c.is_ascii_digit() || c == '.');
        if is_desc {
            i += 1;
            continue;
        }

        // tok is the URL (strip trailing comma).
        let url = tok.trim_end_matches(',').to_string();
        // Look ahead for a width/density descriptor (also strip trailing comma).
        let mut width: Option<u32> = None;
        if i + 1 < tokens.len() {
            let next = tokens[i + 1].trim_end_matches(',');
            if let Some(n_str) = next.strip_suffix('w') {
                width = n_str.parse::<u32>().ok();
            }
        }

        if let Some(w) = width {
            if best_w.is_none_or(|bw| w > bw) {
                best_w = Some(w);
                best_url = Some(url);
            }
        } else if best_url.is_none() {
            best_url = Some(url);
        }

        // Skip the descriptor token if we consumed it.
        if i + 1 < tokens.len() {
            let next = tokens[i + 1].trim_end_matches(',');
            if (next.ends_with('w') || next.ends_with('x'))
                && next[..next.len() - 1]
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
            {
                i += 2;
                continue;
            }
        }
        i += 1;
    }

    best_url
}

/// Detect a `<figure>` whose contents are *not* a simple image+caption — these
/// should be treated as a generic block container and recursed into.
pub fn figure_is_content_wrapper(figure: &NodeRef) -> bool {
    let mut has_img = false;
    let mut has_p_outside_caption = false;
    for child in figure.descendants() {
        if !child.as_element().is_some() {
            continue;
        }
        if is_tag(&child, "img") {
            has_img = true;
        }
        if is_tag(&child, "p") {
            // Check if this `<p>` is inside a figcaption ancestor.
            let mut cur = child.parent();
            let mut in_caption = false;
            while let Some(p) = cur {
                if is_tag(&p, "figcaption") {
                    in_caption = true;
                    break;
                }
                if std::ptr::eq(p.0.as_ref(), figure.0.as_ref()) {
                    break;
                }
                cur = p.parent();
            }
            if !in_caption {
                has_p_outside_caption = true;
            }
        }
    }
    !has_img || has_p_outside_caption
}
