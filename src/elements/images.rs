//! Image normalization (Track D).
//!
//! Defuddle ports `images.ts` here, trimmed to the cases that actually
//! move the body fixture pass-rate:
//!
//! 1. Promote `data-src` / `data-original` / `data-lazy-src` / `data-srcset`
//!    to `src` / `srcset` when `src` is empty or a base64 placeholder.
//! 2. Pick the highest-width entry from `srcset` (whitespace-tokenized so
//!    CDN URLs containing commas aren't split).
//! 3. Strip 1×1 tracking pixels (small explicit width/height OR a known
//!    tracking-pixel substring in the URL).

use kuchikiki::NodeRef;

use super::util::{
    attr, descendants_elements, is_tag, new_element, remove_attr, select_all, set_attr,
    transfer_children,
};
use kuchikiki::traits::TendrilSink;

/// Normalize images in `root`.
pub fn normalize_images(root: &NodeRef) {
    // Promote `<noscript>` wrappers that contain a real image when the
    // surrounding content has only a placeholder image. This is the
    // Next.js / lazyload pattern.
    promote_noscript_images(root);

    let imgs = select_all(root, "img");
    for img in imgs {
        // Lazy-load promotion.
        promote_lazy(&img);

        // Tracking-pixel detection (after promotion, in case the real src
        // was hidden in data-src).
        if is_tracking_pixel(&img) {
            img.detach();
            continue;
        }

        // Resolve srcset → highest-width src, if src is missing or a placeholder.
        if needs_src_from_srcset(&img) {
            if let Some(srcset) = attr(&img, "srcset") {
                if let Some(best) = pick_best_from_srcset(&srcset) {
                    set_attr(&img, "src", &best);
                }
            }
        }
    }
}

/// HTML-string-level pass that promotes `<noscript><img ...></noscript>`
/// content out of the noscript so it survives the lol_html clutter
/// removal pass (which drops noscript wholesale). Operates on the raw
/// HTML *before* clutter removal.
///
/// Strategy: regex-replace `<noscript>...<img.../>...</noscript>` with
/// the contained `<img>` (preserving its attributes verbatim). If the
/// noscript contains multiple imgs, all are promoted in order.
#[must_use]
pub fn promote_noscript_html(html: &str) -> String {
    use once_cell::sync::Lazy;
    use regex::Regex;
    static NOSCRIPT: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"(?is)<noscript[^>]*>(.*?)</noscript>").expect("noscript regex")
    });
    static IMG: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?is)<img\b[^>]*/?>").expect("img regex"));

    NOSCRIPT
        .replace_all(html, |caps: &regex::Captures| {
            let inner = &caps[1];
            // Pull out all <img> tags from the inner content.
            let imgs: Vec<&str> = IMG.find_iter(inner).map(|m| m.as_str()).collect();
            if imgs.is_empty() {
                return caps[0].to_string();
            }
            imgs.join("\n")
        })
        .into_owned()
}

/// Promote `<noscript><img ...></noscript>` content. When a noscript wraps
/// a real image and a sibling/parent placeholder exists, replace the
/// placeholder; otherwise unwrap the noscript so the real img survives.
fn promote_noscript_images(root: &NodeRef) {
    let noscripts = select_all(root, "noscript");
    for ns in noscripts {
        // The noscript element's text-content holds the inner HTML when it
        // was parsed in scripting-enabled mode. Re-parse and inspect.
        let raw = ns.text_contents();
        if !raw.contains("<img") {
            // No image inside — skip.
            continue;
        }
        // Parse the inner HTML.
        let inner_dom = kuchikiki::parse_html()
            .one(format!("<html><body>{raw}</body></html>").as_str());
        let inner_imgs: Vec<NodeRef> = inner_dom
            .descendants()
            .filter(|d| is_tag(d, "img"))
            .collect();
        if inner_imgs.is_empty() {
            continue;
        }
        // Use the FIRST inner img as our promoted candidate.
        let chosen = inner_imgs[0].clone();
        chosen.detach();

        // Decide whether to replace a sibling placeholder or simply
        // unwrap. The pattern: parent contains a real placeholder <img>
        // (data: src / has data-nimg) followed by this noscript.
        let parent = ns.parent();
        let mut replaced = false;
        if let Some(par) = parent.clone() {
            // Find a placeholder img sibling.
            for sib in par.children() {
                if is_tag(&sib, "img") {
                    let s = attr(&sib, "src").unwrap_or_default();
                    let is_placeholder = s.is_empty() || is_base64_placeholder(&s);
                    if is_placeholder {
                        sib.insert_before(chosen.clone());
                        sib.detach();
                        replaced = true;
                        break;
                    }
                }
            }
        }
        if !replaced {
            ns.insert_before(chosen);
        }
        ns.detach();
    }
    // Silence unused warnings for new imports if patterns above don't use
    // some of them yet.
    let _ = (new_element, transfer_children);
}

fn promote_lazy(img: &NodeRef) {
    let src = attr(img, "src").unwrap_or_default();
    let needs_promote = src.is_empty() || is_base64_placeholder(&src);

    let lazy_keys = ["data-src", "data-original", "data-lazy-src"];
    if needs_promote {
        for key in &lazy_keys {
            if let Some(v) = attr(img, key) {
                if !v.is_empty() {
                    set_attr(img, "src", &v);
                    break;
                }
            }
        }
    }

    if attr(img, "srcset").is_none() {
        for key in ["data-srcset", "data-lazy-srcset"] {
            if let Some(v) = attr(img, key) {
                if !v.is_empty() {
                    set_attr(img, "srcset", &v);
                    break;
                }
            }
        }
    }

    // Strip lazy-related attrs.
    for key in [
        "data-src",
        "data-original",
        "data-lazy-src",
        "data-srcset",
        "data-lazy-srcset",
        "loading",
    ] {
        remove_attr(img, key);
    }
}

fn needs_src_from_srcset(img: &NodeRef) -> bool {
    let src = attr(img, "src").unwrap_or_default();
    src.is_empty() || is_base64_placeholder(&src)
}

/// Cheap base64 placeholder detection (small data: URLs).
pub fn is_base64_placeholder(src: &str) -> bool {
    if !src.starts_with("data:") {
        return false;
    }
    // Very small data URLs are treated as placeholders.
    src.len() <= 200
}

/// True if the image looks like a tracking pixel.
fn is_tracking_pixel(img: &NodeRef) -> bool {
    // Explicit 1x1.
    let w = attr(img, "width").and_then(|v| v.parse::<u32>().ok());
    let h = attr(img, "height").and_then(|v| v.parse::<u32>().ok());
    if matches!((w, h), (Some(1), Some(1))) {
        return true;
    }
    if let Some(src) = attr(img, "src") {
        let lower = src.to_lowercase();
        if lower.contains("/pixel.") || lower.contains("tracking") || lower.contains("/1x1.") {
            return true;
        }
    }
    false
}

/// Pick the URL with the largest width descriptor from a srcset string.
/// Whitespace-tokenized so CDN URLs containing `,` aren't split.
pub fn pick_best_from_srcset(srcset: &str) -> Option<String> {
    // We can't naïvely split on commas — substack-style URLs contain `,`
    // inside path segments. Use a tokenizer that walks chars and tracks
    // parens to find the comma+space (or tab) that separates entries.
    let entries = split_srcset_entries(srcset);
    let mut best: Option<(u64, String)> = None;
    for entry in entries {
        // entry is "URL DESCRIPTOR" or just "URL".
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.splitn(2, char::is_whitespace);
        let url = parts.next().unwrap_or("").trim().to_string();
        let descriptor = parts.next().unwrap_or("").trim();
        let weight = parse_descriptor_weight(descriptor);
        if url.is_empty() {
            continue;
        }
        match &best {
            Some((w, _)) if *w >= weight => {}
            _ => best = Some((weight, url)),
        }
    }
    best.map(|(_, u)| u)
}

fn split_srcset_entries(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_url = true;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_url {
            if c.is_whitespace() {
                in_url = false;
                cur.push(c);
            } else {
                cur.push(c);
            }
        } else {
            // After we've seen whitespace post-URL, the next "," is a
            // candidate separator. Confirm it's followed by whitespace
            // or end-of-string before treating as a separator.
            if c == ',' {
                let next_is_ws = chars.get(i + 1).map(|c| c.is_whitespace()).unwrap_or(true);
                if next_is_ws {
                    out.push(cur.trim().to_string());
                    cur.clear();
                    in_url = true;
                    i += 1;
                    // Skip the whitespace right after.
                    while i < chars.len() && chars[i].is_whitespace() {
                        i += 1;
                    }
                    continue;
                }
            }
            cur.push(c);
        }
        i += 1;
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

fn parse_descriptor_weight(d: &str) -> u64 {
    let d = d.trim();
    if d.is_empty() {
        return 0;
    }
    if let Some(num) = d.strip_suffix('w') {
        return num.trim().parse::<u64>().unwrap_or(0);
    }
    if let Some(num) = d.strip_suffix('x') {
        let f: f64 = num.trim().parse().unwrap_or(0.0);
        return (f * 1000.0) as u64;
    }
    0
}

/// Convenience accessor used by tests.
pub fn extract_first_url_from_srcset(srcset: &str) -> Option<String> {
    split_srcset_entries(srcset)
        .into_iter()
        .find_map(|e| e.split_whitespace().next().map(String::from))
}

#[allow(dead_code)]
fn _keep_imports(_n: &NodeRef) {
    let _ = (descendants_elements, is_tag);
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
    fn promotes_data_src() {
        let html = r#"<html><body><img src="" data-src="real.png"></body></html>"#;
        let root = parse(html);
        normalize_images(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"src="real.png""#), "got: {out}");
        assert!(!out.contains("data-src"), "got: {out}");
    }

    #[test]
    fn picks_highest_width_from_srcset() {
        let s = "a.png 100w, b.png 800w, c.png 400w";
        assert_eq!(pick_best_from_srcset(s).as_deref(), Some("b.png"));
    }

    #[test]
    fn srcset_tolerates_commas_in_url() {
        let s =
            "https://cdn.example/path,foo/img.png 800w, https://cdn.example/path,bar/img.png 1600w";
        let best = pick_best_from_srcset(s).unwrap();
        assert!(best.contains("path,bar"), "got: {best}");
    }

    #[test]
    fn tracking_pixel_is_dropped() {
        let html = r#"<html><body><img src="/pixel.gif" width="1" height="1"></body></html>"#;
        let root = parse(html);
        normalize_images(&root);
        let out = serialize(&root);
        assert!(!out.contains("img"), "got: {out}");
    }
}
