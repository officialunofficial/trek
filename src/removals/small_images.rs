//! Remove tiny images (icons, tracking pixels, base64 placeholders).

use crate::dom::engine::NodeRef;
use regex::Regex;
use std::sync::LazyLock;

use crate::dom::walk::{closest_tag, get_attr, is_any_tag};
use crate::dom::{DomCtx, DomPass};

pub struct SmallImages;

const MIN_DIMENSION: u32 = 33;

static STYLE_W: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)width\s*:\s*(\d+)").expect("valid regex"));
static STYLE_H: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)height\s*:\s*(\d+)").expect("valid regex"));

fn parse_u32(s: &str) -> u32 {
    s.parse::<u32>().unwrap_or(0)
}

fn dimension_from_attrs(node: &NodeRef) -> (u32, u32) {
    let w = get_attr(node, "width").map_or(0, |v| parse_u32(&v));
    let h = get_attr(node, "height").map_or(0, |v| parse_u32(&v));
    (w, h)
}

fn dimension_from_style(node: &NodeRef) -> (u32, u32) {
    let style = get_attr(node, "style").unwrap_or_default();
    let w = STYLE_W
        .captures(&style)
        .and_then(|c| c.get(1))
        .map_or(0, |m| parse_u32(m.as_str()));
    let h = STYLE_H
        .captures(&style)
        .and_then(|c| c.get(1))
        .map_or(0, |m| parse_u32(m.as_str()));
    (w, h)
}

/// Converts a parsed SVG `viewBox` dimension to `u32`, clamping to a safe,
/// non-negative range first so the final cast cannot truncate or flip sign.
fn f64_to_u32(v: f64) -> u32 {
    let rounded = v.round();
    if !rounded.is_finite() || rounded <= 0.0 {
        return 0;
    }
    let clamped = rounded.min(f64::from(u32::MAX));
    // Safe: `clamped` is finite, non-negative, and at most `u32::MAX` here.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        clamped as u32
    }
}

fn dimension_from_viewbox(node: &NodeRef) -> (u32, u32) {
    if !is_any_tag(node, &["svg"]) {
        return (0, 0);
    }
    let vb = get_attr(node, "viewBox").unwrap_or_default();
    let parts: Vec<&str> = vb
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.len() == 4 {
        let w = f64_to_u32(parts[2].parse::<f64>().unwrap_or(0.0));
        let h = f64_to_u32(parts[3].parse::<f64>().unwrap_or(0.0));
        return (w, h);
    }
    (0, 0)
}

fn looks_like_math(node: &NodeRef) -> bool {
    if let Some(alt) = get_attr(node, "alt") {
        let a = alt.to_ascii_lowercase();
        if a.contains("\\(") || a.contains("\\[") || a.starts_with('$') || a.contains("latex") {
            return true;
        }
    }
    if let Some(class) = get_attr(node, "class") {
        let lc = class.to_ascii_lowercase();
        if lc.contains("latex")
            || lc.contains("tex")
            || lc.contains("equation")
            || lc.contains("math")
        {
            return true;
        }
    }
    if get_attr(node, "data-latex").is_some() || get_attr(node, "data-math").is_some() {
        return true;
    }
    false
}

fn is_base64_placeholder(src: &str) -> bool {
    if !src.starts_with("data:") {
        return false;
    }
    src.len() < 300
}

impl DomPass for SmallImages {
    fn name(&self) -> &'static str {
        "small_images"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        let mut to_remove: Vec<NodeRef> = Vec::new();
        for d in root.descendants() {
            if !is_any_tag(&d, &["img", "svg"]) {
                continue;
            }

            // Skip math
            if looks_like_math(&d) {
                continue;
            }

            // Skip the sole image inside a <figure> (probably the figure subject).
            if is_any_tag(&d, &["img"]) {
                if let Some(figure) = closest_tag(&d, &["figure"]) {
                    // Only skip when the figure has exactly one image.
                    let img_count = figure
                        .descendants()
                        .filter(|x| is_any_tag(x, &["img"]))
                        .count();
                    if img_count == 1 {
                        continue;
                    }
                }
            }

            // Compute dimensions.
            let (aw, ah) = dimension_from_attrs(&d);
            let (sw, sh) = dimension_from_style(&d);
            let (vw, vh) = dimension_from_viewbox(&d);
            let widths = [aw, sw, vw]
                .into_iter()
                .filter(|x| *x > 0)
                .collect::<Vec<_>>();
            let heights = [ah, sh, vh]
                .into_iter()
                .filter(|x| *x > 0)
                .collect::<Vec<_>>();
            if widths.is_empty() && heights.is_empty() {
                // Broken images on <img> with no src/alt-src → drop.
                if is_any_tag(&d, &["img"]) {
                    let src = get_attr(&d, "src").unwrap_or_default();
                    let has_alt_src = [
                        "srcset",
                        "data-src",
                        "data-srcset",
                        "data-lazy-src",
                        "data-original",
                    ]
                    .iter()
                    .any(|k| get_attr(&d, k).is_some_and(|v| !v.is_empty()));
                    if src.is_empty() && !has_alt_src {
                        to_remove.push(d.clone());
                        continue;
                    }
                    if !has_alt_src
                        && is_base64_placeholder(&src)
                        && closest_tag(&d, &["picture"]).is_none()
                    {
                        to_remove.push(d.clone());
                        continue;
                    }
                }
                continue;
            }
            let min_w = widths.iter().copied().min().unwrap_or(u32::MAX);
            let min_h = heights.iter().copied().min().unwrap_or(u32::MAX);
            if min_w < MIN_DIMENSION || min_h < MIN_DIMENSION {
                to_remove.push(d.clone());
            }
        }
        for n in to_remove {
            if n.parent().is_some() {
                n.detach();
            }
        }
    }
}
