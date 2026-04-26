//! Math base — analyzers + raw-LaTeX wrapper (Track D).
//!
//! Defuddle ports `math.base.ts` here. The base tier covers the cheap,
//! dependency-free conversions:
//!
//! * MathML elements with `alttext` → `<span data-math="$alttext$">…</span>`.
//! * KaTeX wrappers (`<span class="katex">…<annotation
//!   encoding="application/x-tex">FORMULA</annotation>…</span>`) →
//!   `<span data-math="$FORMULA$">FORMULA</span>`.
//! * Display math (recognized via class / nesting) is wrapped with `$$ … $$`
//!   instead of `$ … $`.
//!
//! Markdown rendering for `<span data-math>` happens in
//! `src/markdown/mod.rs` already (it falls through to inline text), so we
//! emit literal text containing the delimiters. This is the same shape
//! Defuddle's `math.core` produces for the browser-core bundle.

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::elements::util::{attr, has_class, is_tag, new_element, select_all, set_attr};

/// Run the math-base normalization pass over `root`.
pub fn normalize_math_base(root: &NodeRef) {
    // Wrap raw `$…$` / `$$…$$` / `\(…\)` / `\[…\]` text into `<math>`
    // elements first so the existing math pipeline sees them.
    wrap_raw_latex_delimiters(root);
    // Promote LaTeX-rendering images (CodeCogs, Google Charts, alt-as-LaTeX
    // wp-images, etc.) into `<math>` elements.
    process_latex_images(root);
    // Spans carrying `data-math` (Gemini-style `<span class="math-inline">`
    // / `<span class="math-block">`).
    process_data_math_spans(root);
    process_mathml(root);
    process_katex(root);
}

/// Detect block (display) math.
pub fn is_block_display(el: &NodeRef) -> bool {
    if attr(el, "display").as_deref() == Some("block") {
        return true;
    }
    if has_class(el, "katex-display") || has_class(el, "math-display") {
        return true;
    }
    if let Some(class) = attr(el, "class") {
        if class
            .split_whitespace()
            .any(|c| c == "display" || c == "block")
        {
            return true;
        }
    }
    // Walk ancestors looking for a known display container.
    let mut cur = el.parent();
    while let Some(p) = cur {
        if has_class(&p, "katex-display") || has_class(&p, "MathJax_Display") {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// Replace `node` with a fresh element node.
fn replace_with(old: &NodeRef, new: NodeRef) {
    old.insert_before(new);
    old.detach();
}

// ---------------------------------------------------------------------------
// MathML
// ---------------------------------------------------------------------------

fn process_mathml(root: &NodeRef) {
    // Direct MathML elements.
    let nodes: Vec<NodeRef> = root.descendants().filter(|n| is_tag(n, "math")).collect();

    for el in nodes {
        // Promote a TeX annotation into `alttext` if `alttext` is missing.
        if attr(&el, "alttext").is_none() {
            if let Some(latex) = el
                .descendants()
                .find(|n| {
                    is_tag(n, "annotation")
                        && attr(n, "encoding").as_deref() == Some("application/x-tex")
                })
                .map(|n| n.text_contents().trim().to_string())
                .filter(|s| !s.is_empty())
            {
                set_attr(&el, "alttext", &latex);
            }
        }

        // Promote ancestor block display onto the element so the markdown
        // renderer emits $$…$$. Don't replace the <math> element — leave
        // it in place so the renderer's math handler runs.
        if attr(&el, "display").as_deref() != Some("block") && is_block_display(&el) {
            set_attr(&el, "display", "block");
        }
    }
}
// ---------------------------------------------------------------------------
// KaTeX
// ---------------------------------------------------------------------------

fn process_katex(root: &NodeRef) {
    // Annotate `.katex` / `.katex-display` wrappers so the markdown renderer
    // can pick the LaTeX up via `data-latex`. The renderer already handles
    // these spans (see `markdown::mod` `katex_latex`) — we just need to
    // hoist annotations into a stable attribute.
    let nodes = select_all(root, ".katex, .katex-display");
    let mut handled: Vec<*const kuchikiki::Node> = Vec::new();
    for el in nodes {
        let ptr = std::rc::Rc::as_ptr(&el.0).cast::<kuchikiki::Node>();
        if handled.iter().any(|h| std::ptr::eq(*h, ptr)) {
            continue;
        }
        // Skip if any ancestor is also being handled (avoid double-process).
        let mut anc = el.parent();
        let mut skip = false;
        while let Some(p) = anc {
            if has_class(&p, "katex") || has_class(&p, "katex-display") {
                skip = true;
                break;
            }
            anc = p.parent();
        }
        if skip {
            continue;
        }

        let latex = attr(&el, "data-latex").filter(|s| !s.is_empty()).or_else(|| {
            el.descendants()
                .find(|n| {
                    is_tag(n, "annotation")
                        && attr(n, "encoding").as_deref() == Some("application/x-tex")
                })
                .map(|n| n.text_contents().trim().to_string())
                .filter(|s| !s.is_empty())
        });

        let Some(latex) = latex else {
            continue;
        };
        for d in el.descendants() {
            if has_class(&d, "katex") || has_class(&d, "katex-display") {
                handled.push(std::rc::Rc::as_ptr(&d.0).cast::<kuchikiki::Node>());
            }
        }
        // Set `data-latex` so markdown rendering reads from a stable spot.
        if attr(&el, "data-latex").is_none() {
            set_attr(&el, "data-latex", &latex);
        }
        // Mirror display state — if the wrapper is a `.katex-display`, mark
        // its inner `.katex` (if any) so block detection survives unwrapping.
        let block = has_class(&el, "katex-display") || is_block_display(&el);
        if block {
            // Add `math-display` class hint that markdown renderer recognises.
            let cls = attr(&el, "class").unwrap_or_default();
            if !cls.split_whitespace().any(|c| c == "math-display") {
                let new_cls = if cls.is_empty() {
                    "math-display".to_string()
                } else {
                    format!("{cls} math-display")
                };
                set_attr(&el, "class", &new_cls);
            }
        }
    }
}

/// Wrap a LaTeX formula in `$…$` (inline) or `$$…$$` (block).
fn wrap_delimiters(latex: &str, block: bool) -> String {
    let trimmed = latex.trim();
    if block {
        format!("$${}$$", trimmed)
    } else {
        format!("${}$", trimmed)
    }
}

// ---------------------------------------------------------------------------
// LaTeX-image rendering services
// ---------------------------------------------------------------------------

static LATEX_PARAM_RE: Lazy<[Regex; 5]> = Lazy::new(|| {
    [
        Regex::new(r"(?i)[?&]latex=([^&#]+)").expect("latex param"),
        Regex::new(r"(?i)[?&]chl=([^&#]+)").expect("chl param"),
        Regex::new(r"(?i)[?&]tex=([^&#]+)").expect("tex param"),
        Regex::new(r"(?i)[?&]eq=([^&#]+)").expect("eq param"),
        Regex::new(r"(?i)[?&]math=([^&#]+)").expect("math param"),
    ]
});

static LOOKS_LIKE_LATEX_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\\[a-zA-Z]{2,}").expect("looks like latex"));

fn percent_decode(s: &str) -> Option<String> {
    // Replace '+' with space, then decode %XX sequences.
    let pre = s.replace('+', " ");
    let bytes = pre.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16)?;
            let lo = (bytes[i + 2] as char).to_digit(16)?;
            out.push(((hi << 4) | lo) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn decode_latex_segment(raw: &str) -> Option<String> {
    let decoded = percent_decode(raw)?;
    if LOOKS_LIKE_LATEX_RE.is_match(&decoded) {
        Some(decoded)
    } else {
        None
    }
}

/// Extract LaTeX from an image src URL.
fn extract_latex_from_image_src(src: &str) -> Option<String> {
    // Try named query parameters.
    for re in LATEX_PARAM_RE.iter() {
        if let Some(caps) = re.captures(src) {
            if let Some(m) = caps.get(1) {
                if let Some(s) = decode_latex_segment(m.as_str()) {
                    return Some(s);
                }
            }
        }
    }
    // Bare query string (e.g. `latex.codecogs.com/svg.image?%5Cfrac…`).
    if let Some(q_idx) = src.find('?') {
        let q_end = src[q_idx + 1..].find('#').map_or(src.len(), |i| q_idx + 1 + i);
        let q = &src[q_idx + 1..q_end];
        if !q.contains('=') {
            if let Some(s) = decode_latex_segment(q) {
                return Some(s);
            }
        } else {
            // Some services use `?key=` only as the LaTeX itself (rare).
            if let Some(s) = decode_latex_segment(q) {
                return Some(s);
            }
        }
    }
    // Try URL path segments containing %5C (encoded backslash).
    let path = src.split('?').next().unwrap_or(src);
    for seg in path.rsplit('/') {
        if seg.to_ascii_lowercase().contains("%5c") {
            if let Some(s) = decode_latex_segment(seg) {
                return Some(s);
            }
        }
    }
    None
}

fn looks_like_latex(s: &str) -> bool {
    LOOKS_LIKE_LATEX_RE.is_match(s) || s.contains('_') && s.contains('^')
}

fn is_mediawiki_fallback_image(img: &NodeRef) -> bool {
    // MediaWiki wraps the `<math>` and the fallback `<img>` together in
    // `<span class="mwe-math-element">`. If we're inside such a wrapper
    // and it also contains a `<math>` element, the `<img>` is a duplicate.
    let cls = attr(img, "class").unwrap_or_default();
    if cls.split_whitespace().any(|c| {
        c == "mwe-math-fallback-image-inline"
            || c == "mwe-math-fallback-image-display"
            || c == "mwe-math-fallback-image"
    }) {
        return true;
    }
    let mut anc = img.parent();
    let mut depth = 0usize;
    while let Some(p) = anc {
        if depth >= 3 {
            break;
        }
        let pcls = attr(&p, "class").unwrap_or_default();
        if pcls.split_whitespace().any(|c| c == "mwe-math-element") {
            // Look for a math element inside this wrapper.
            for d in p.descendants() {
                if is_tag(&d, "math") {
                    return true;
                }
            }
        }
        depth += 1;
        anc = p.parent();
    }
    false
}

fn process_latex_images(root: &NodeRef) {
    let imgs = select_all(root, "img");
    for img in imgs {
        if !is_tag(&img, "img") {
            continue;
        }
        // MediaWiki fallback images: when the hidden MathML span is still
        // present we'd produce a duplicate, so detach the image. When the
        // hidden span has already been removed by the `hidden` pass, fall
        // through to the standard alt-text → LaTeX conversion below.
        if is_mediawiki_fallback_image(&img) {
            // Look for a sibling `<math>` in the wrapper.
            let mut has_math = false;
            let mut anc = img.parent();
            for _ in 0..3 {
                let Some(p) = anc.clone() else { break };
                if p.descendants().any(|n| is_tag(&n, "math")) {
                    has_math = true;
                    break;
                }
                anc = p.parent();
            }
            if has_math {
                img.detach();
                continue;
            }
            // No surviving <math>: convert this img.
        }
        // Prefer `alt` text when it looks like LaTeX (CodeCogs renderers,
        // wp-latex, MediaWiki fallback images).
        let alt = attr(&img, "alt").unwrap_or_default();
        let src = attr(&img, "src").unwrap_or_default();

        let latex_from_alt = if !alt.trim().is_empty() && looks_like_latex(&alt) {
            Some(alt.clone())
        } else {
            None
        };
        let latex_from_src = if !src.is_empty() {
            extract_latex_from_image_src(&src)
        } else {
            None
        };

        let Some(latex) = latex_from_alt.or(latex_from_src) else {
            continue;
        };
        let trimmed = latex.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        // Block detection: explicit class hint, `block` in class, or an
        // image whose `<p>` parent contains nothing else (the typical
        // wp-style "centered equation" pattern).
        let class = attr(&img, "class").unwrap_or_default();
        let class_lower = class.to_ascii_lowercase();
        let mut block = class_lower.contains("display")
            || class_lower.contains("block")
            || class_lower.contains("aligncenter");
        // If the parent `<p>` contains only this image (no surrounding text),
        // treat as block.
        if !block {
            if let Some(parent) = img.parent() {
                if is_tag(&parent, "p") {
                    let parent_only_img = parent.children().all(|c| {
                        if let Some(t) = c.as_text() {
                            t.borrow().trim().is_empty()
                        } else if c.as_element().is_some() {
                            std::ptr::eq(
                                std::rc::Rc::as_ptr(&c.0).cast::<()>(),
                                std::rc::Rc::as_ptr(&img.0).cast::<()>(),
                            )
                        } else {
                            true
                        }
                    });
                    if parent_only_img {
                        block = true;
                    }
                }
            }
        }

        // Build `<math display=… alttext="…">latex</math>` and replace.
        let display = if block { "block" } else { "inline" };
        let math_el = new_element(
            "math",
            &[
                ("xmlns", "http://www.w3.org/1998/Math/MathML"),
                ("display", display),
                ("data-latex", &trimmed),
                ("alttext", &trimmed),
            ],
        );
        math_el.append(NodeRef::new_text(trimmed.clone()));
        replace_with(&img, math_el);
    }
}

// ---------------------------------------------------------------------------
// data-math span → <math>
// ---------------------------------------------------------------------------

fn process_data_math_spans(root: &NodeRef) {
    let nodes = select_all(root, "span[data-math], div[data-math]");
    for el in nodes {
        let latex = match attr(&el, "data-math") {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => continue,
        };
        let cls = attr(&el, "class").unwrap_or_default();
        let block = cls.split_whitespace().any(|c| c == "math-block")
            || attr(&el, "data-display").as_deref() == Some("block");
        let display = if block { "block" } else { "inline" };
        let math_el = new_element(
            "math",
            &[
                ("xmlns", "http://www.w3.org/1998/Math/MathML"),
                ("display", display),
                ("data-latex", &latex),
                ("alttext", &latex),
            ],
        );
        math_el.append(NodeRef::new_text(latex));
        replace_with(&el, math_el);
    }
}

// ---------------------------------------------------------------------------
// Raw LaTeX delimiter wrapping
// ---------------------------------------------------------------------------

// Combined LaTeX delimiter regex. Order: $$..$$, \[..\], $..$, \(..\).
static LATEX_DELIM_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?s)\$\$(.+?)\$\$|\\\[(.+?)\\\]|\$([^\s$][^$]*[^\s$]|[^\s$])\$|\\\((.+?)\\\)",
    )
    .expect("LaTeX delim regex")
});

const LATEX_CMD_RE: &str = r"\\[a-zA-Z]";
const LATEX_STRUCT_CHARS: &[char] = &['_', '^', '{', '}'];

fn contains_latex_command(s: &str) -> bool {
    static LATEX_CMD: Lazy<Regex> =
        Lazy::new(|| Regex::new(LATEX_CMD_RE).expect("latex cmd regex"));
    if LATEX_CMD.is_match(s) {
        return true;
    }
    s.chars().any(|c| LATEX_STRUCT_CHARS.contains(&c))
}

/// True if the document includes a MathJax or KaTeX `<script>` tag.
fn has_math_library(root: &NodeRef) -> bool {
    for s in select_all(root, "script") {
        if let Some(src) = attr(&s, "src") {
            let lc = src.to_ascii_lowercase();
            if lc.contains("mathjax") || lc.contains("katex") {
                return true;
            }
        } else {
            let txt = s.text_contents();
            if txt.contains("MathJax") || txt.to_ascii_lowercase().contains("katex") {
                return true;
            }
        }
    }
    false
}

const RAW_LATEX_SKIP_TAGS: &[&str] = &[
    "pre", "code", "script", "style", "math", "svg", "textarea",
];

fn is_inside_skip_tag(node: &NodeRef) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if let Some(name) = p
            .as_element()
            .map(|e| e.name.local.to_string().to_ascii_lowercase())
        {
            if RAW_LATEX_SKIP_TAGS.iter().any(|t| *t == name.as_str()) {
                return true;
            }
        }
        cur = p.parent();
    }
    false
}

#[derive(Debug)]
enum LatexPart {
    Text(String),
    Math { latex: String, block: bool },
}

fn wrap_raw_latex_delimiters(root: &NodeRef) {
    // Note: defuddle gates this on the presence of a MathJax/KaTeX
    // `<script>` tag, but our pipeline strips scripts before this pass
    // runs. We instead rely on the combination of the explicit-command
    // heuristic (only `$…$` containing a `\foo` or `_^{}`) and unambiguous
    // backslash delimiters (`\(…\)`, `\[…\]`) to keep currency text safe.
    let _ = has_math_library; // keep defuddle parity helper available

    // Skip if document already has rendered math (other passes will handle).
    let already_has_math = select_all(root, "math, mjx-container, .MathJax, .katex, [data-math], [data-latex]")
        .into_iter()
        .next()
        .is_some();
    if already_has_math {
        return;
    }

    // Collect text nodes (excluding ones inside skip tags) ahead of mutation.
    let text_nodes: Vec<NodeRef> = root
        .descendants()
        .filter(|n| n.as_text().is_some())
        .filter(|n| !is_inside_skip_tag(n))
        .collect();

    for text_node in text_nodes {
        let text = text_node.as_text().map(|t| t.borrow().clone()).unwrap_or_default();
        if !text.contains('$') && !text.contains("\\(") && !text.contains("\\[") {
            continue;
        }

        // Build parts.
        let mut parts: Vec<LatexPart> = Vec::new();
        let mut last_end = 0usize;
        let mut had_block = false;

        for caps in LATEX_DELIM_RE.captures_iter(&text) {
            let m = caps.get(0).expect("group 0");
            let block_content = caps.get(1).or_else(|| caps.get(2));
            let inline_content = caps.get(3).or_else(|| caps.get(4));
            let is_backslash = caps.get(2).is_some() || caps.get(4).is_some();
            let is_block = block_content.is_some();
            let raw_latex = block_content.or(inline_content).map(|x| x.as_str()).unwrap_or("");
            let latex = raw_latex.trim().to_string();
            if latex.is_empty() {
                continue;
            }
            if !is_backslash && !contains_latex_command(&latex) {
                continue;
            }

            if last_end < m.start() {
                parts.push(LatexPart::Text(text[last_end..m.start()].to_string()));
            }
            if is_block {
                had_block = true;
            }
            parts.push(LatexPart::Math {
                latex,
                block: is_block,
            });
            last_end = m.end();
        }
        if parts.is_empty() {
            continue;
        }
        if last_end < text.len() {
            parts.push(LatexPart::Text(text[last_end..].to_string()));
        }

        // Force inline if there's surrounding text or sibling content in the
        // same parent (block math should be the entire content of a paragraph).
        if had_block {
            let has_text_around = parts
                .iter()
                .any(|p| matches!(p, LatexPart::Text(s) if !s.trim().is_empty()));
            let parent_has_other = text_node
                .parent()
                .map(|p| {
                    p.children().any(|c| {
                        if std::ptr::eq(
                            std::rc::Rc::as_ptr(&c.0).cast::<()>(),
                            std::rc::Rc::as_ptr(&text_node.0).cast::<()>(),
                        ) {
                            return false;
                        }
                        if let Some(t) = c.as_text() {
                            return !t.borrow().trim().is_empty();
                        }
                        c.as_element().is_some()
                    })
                })
                .unwrap_or(false);
            if has_text_around || parent_has_other {
                for p in parts.iter_mut() {
                    if let LatexPart::Math { block, .. } = p {
                        *block = false;
                    }
                }
            }
        }

        // Insert parts before the original text node, then detach it.
        for part in parts {
            match part {
                LatexPart::Text(s) => {
                    text_node.insert_before(NodeRef::new_text(s));
                }
                LatexPart::Math { latex, block } => {
                    let display = if block { "block" } else { "inline" };
                    let math_el = new_element(
                        "math",
                        &[
                            ("xmlns", "http://www.w3.org/1998/Math/MathML"),
                            ("display", display),
                            ("data-latex", &latex),
                            ("alttext", &latex),
                        ],
                    );
                    math_el.append(NodeRef::new_text(latex.clone()));
                    // Ensure alttext attribute is set (so process_mathml picks it up).
                    set_attr(&math_el, "alttext", &latex);
                    text_node.insert_before(math_el);
                }
            }
        }
        text_node.detach();
    }
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
    fn mathml_alttext_is_preserved() {
        // The pass leaves the `<math>` element in place so the markdown
        // renderer can emit `$alttext$` itself; we just verify the
        // attribute survives the pipeline.
        let html = r#"<html><body><p><math alttext="x^2"><mi>x</mi></math></p></body></html>"#;
        let root = parse(html);
        normalize_math_base(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"alttext="x^2""#), "got: {out}");
        assert!(out.contains("<math"), "got: {out}");
    }

    #[test]
    fn katex_annotation_promotes_data_latex() {
        // `process_katex` lifts the TeX annotation into `data-latex` on the
        // `.katex` wrapper without rewriting the element. Markdown rendering
        // reads from `data-latex` directly.
        let html = r#"<html><body><span class="katex"><annotation encoding="application/x-tex">a+b</annotation></span></body></html>"#;
        let root = parse(html);
        normalize_math_base(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"data-latex="a+b""#), "got: {out}");
    }

    #[test]
    fn katex_display_marks_display_class() {
        let html = r#"<html><body><span class="katex-display"><span class="katex"><annotation encoding="application/x-tex">x^2</annotation></span></span></body></html>"#;
        let root = parse(html);
        normalize_math_base(&root);
        let out = serialize(&root);
        assert!(out.contains("math-display"), "got: {out}");
        assert!(out.contains(r#"data-latex="x^2""#), "got: {out}");
    }

    #[test]
    fn raw_latex_dollars_become_math() {
        let html = r#"<html><body><p>An equation $x^2 + y^2 = z^2$ here.</p></body></html>"#;
        let root = parse(html);
        normalize_math_base(&root);
        let out = serialize(&root);
        assert!(out.contains("<math"), "got: {out}");
        assert!(out.contains("x^2 + y^2 = z^2"), "got: {out}");
    }

    #[test]
    fn raw_latex_backslash_brackets_become_block_math() {
        let html = r#"<html><body><p>\[F = ma\]</p></body></html>"#;
        let root = parse(html);
        normalize_math_base(&root);
        let out = serialize(&root);
        assert!(out.contains("<math"), "got: {out}");
        assert!(out.contains(r#"display="block""#), "got: {out}");
    }

    #[test]
    fn latex_image_alt_becomes_math() {
        let html = r#"<html><body><p><img src="https://example.com/eq.svg" alt="\frac{a}{b}"></p></body></html>"#;
        let root = parse(html);
        normalize_math_base(&root);
        let out = serialize(&root);
        assert!(out.contains("<math"), "got: {out}");
        assert!(out.contains(r#"\frac{a}{b}"#), "got: {out}");
    }

    #[test]
    fn latex_image_url_param_becomes_math() {
        let html = r#"<html><body><p>x<img src="https://latex.codecogs.com/svg.image?%5Cfrac%7Ba%7D%7Bb%7D">y</p></body></html>"#;
        let root = parse(html);
        normalize_math_base(&root);
        let out = serialize(&root);
        assert!(out.contains("<math"), "got: {out}");
        assert!(out.contains(r#"\frac{a}{b}"#), "got: {out}");
    }
}
