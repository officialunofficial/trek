//! Code-block language detection and content extraction.

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use super::util::{attr, is_tag};

static LANG_CLASS_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?:^|\s)(?:lang|language)-([A-Za-z0-9_+\-]+)").expect("regex"));

/// Detect the programming language hint for a `<pre><code>` block.
///
/// Priority (matches Defuddle):
/// 1. `<code data-language="...">` / `<code data-lang="...">`
/// 2. `<code class="language-xxx">` or `lang-xxx`
/// 3. `<pre data-language="...">` / `<pre data-lang="...">`
/// 4. `<pre class="language-xxx">`
pub fn detect_language(pre: &NodeRef, code: Option<&NodeRef>) -> Option<String> {
    if let Some(c) = code {
        if let Some(v) = attr(c, "data-language") {
            return Some(v);
        }
        if let Some(v) = attr(c, "data-lang") {
            return Some(v);
        }
        if let Some(class) = attr(c, "class") {
            if let Some(cap) = LANG_CLASS_RE.captures(&class) {
                return Some(cap[1].to_string());
            }
        }
    }
    if let Some(v) = attr(pre, "data-language") {
        return Some(v);
    }
    if let Some(v) = attr(pre, "data-lang") {
        return Some(v);
    }
    if let Some(class) = attr(pre, "class") {
        if let Some(cap) = LANG_CLASS_RE.captures(&class) {
            return Some(cap[1].to_string());
        }
    }
    None
}

/// Extract the textual content of a code block, preserving line structure
/// while stripping syntax-highlight wrapper elements.
///
/// Shiki / rehype-pretty-code wrap each line in `<span data-line>...`. The
/// expected golden output is plain text, so we use `text_contents` and then
/// normalize the result.
pub fn extract_code_text(pre: &NodeRef) -> String {
    // Find the inner <code> if present, else use the <pre> directly.
    let inner = pre
        .descendants()
        .find(|n| is_tag(n, "code"))
        .unwrap_or_else(|| pre.clone());

    // Walk the descendants of `inner` and join text nodes, inserting newlines
    // when we cross a block-line boundary (each `<span data-line>` is one line
    // in Shiki output, each `<br>` is a line in many highlighters).
    let mut out = String::new();
    walk_code(&inner, &mut out);

    // Normalize: trim leading/trailing whitespace lines but keep internal blanks.
    let trimmed: Vec<&str> = out.lines().collect();
    let mut start = 0usize;
    let mut end = trimmed.len();
    while start < end && trimmed[start].trim().is_empty() {
        start += 1;
    }
    while end > start && trimmed[end - 1].trim().is_empty() {
        end -= 1;
    }
    trimmed[start..end].join("\n")
}

/// Heuristic: a `<span>` whose only text is a small integer is a line-number
/// marker. We require the span to be the first child of its parent (so it's
/// "leftmost" — typical line-gutter layout) and to have a following sibling
/// (so we don't strip a numeric literal that happens to live in its own span).
fn is_line_number_span(node: &NodeRef) -> bool {
    let text = node.text_contents();
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 4 {
        return false;
    }
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Must be the leftmost child of its parent (skipping whitespace text).
    let mut prev = node.previous_sibling();
    while let Some(p) = prev {
        if let Some(t) = p.as_text() {
            if t.borrow().chars().all(char::is_whitespace) {
                prev = p.previous_sibling();
                continue;
            }
        }
        return false;
    }
    // Must have a following non-whitespace sibling.
    let mut next = node.next_sibling();
    while let Some(p) = next {
        if let Some(t) = p.as_text() {
            if t.borrow().chars().all(char::is_whitespace) {
                next = p.next_sibling();
                continue;
            }
            return true;
        }
        if p.as_element().is_some() {
            return true;
        }
        next = p.next_sibling();
    }
    false
}

fn walk_code(node: &NodeRef, out: &mut String) {
    if let Some(text) = node.as_text() {
        out.push_str(&text.borrow());
        return;
    }

    let tag = node
        .as_element()
        .map(|e| e.name.local.to_string().to_ascii_lowercase());

    if let Some(t) = &tag {
        match t.as_str() {
            "br" => {
                out.push('\n');
                return;
            }
            // Strip non-content highlighter chrome: copy buttons, language
            // label headers, etc.
            "button" => return,
            // rehype-pretty-code, Shiki, and friends use <span data-line>
            // for one line of code each. We don't force a newline because
            // the HTML usually contains literal `\n` between line spans;
            // adding our own would double them.
            "span" => {
                // Skip line-number indicators inside spans (Shiki / hexo / etc).
                if attr(node, "data-lineno").is_some()
                    || super::util::has_any_class(
                        node,
                        &[
                            "lineno",
                            "linenos",
                            "line-number",
                            "linenumber",
                            "react-syntax-highlighter-line-number",
                            "ln",
                            "lnt",
                            "gutter",
                        ],
                    )
                    || is_line_number_span(node)
                {
                    return;
                }
                for child in node.children() {
                    walk_code(&child, out);
                }
                return;
            }
            // Skip line-number gutters that some highlighters interleave.
            "code" | "pre" => {} // descend
            "div" => {
                // Each row of a flex-grid code block is a `<div>`. Treat
                // every <div> at code-content level as a line — recurse and
                // ensure a trailing newline.
                if let Some(class) = attr(node, "class") {
                    let lower = class.to_ascii_lowercase();
                    if lower.contains("code__header")
                        || lower.contains("code-header")
                        || lower.contains("codeheader")
                        || lower.contains("copy-button")
                        || lower.contains("copy_button")
                        || lower.contains("code-copy")
                        || lower.contains("language-label")
                    {
                        return;
                    }
                }
                let before_len = out.len();
                for child in node.children() {
                    walk_code(&child, out);
                }
                // Only insert a newline if we actually produced output and
                // it doesn't already end with one.
                if out.len() > before_len && !out.ends_with('\n') {
                    out.push('\n');
                }
                return;
            }
            _ => {
                // Strip wrappers that look like code-block chrome (header,
                // copy button, language label, etc).
                if let Some(class) = attr(node, "class") {
                    let lower = class.to_ascii_lowercase();
                    if lower.contains("code__header")
                        || lower.contains("code-header")
                        || lower.contains("codeheader")
                        || lower.contains("copy-button")
                        || lower.contains("copy_button")
                        || lower.contains("code-copy")
                        || lower.contains("language-label")
                        || lower.contains("__lang")
                    {
                        return;
                    }
                }
                if super::util::has_any_class(
                    node,
                    &["lineno", "linenos", "line-number", "ln", "gutter"],
                ) {
                    return;
                }
            }
        }
    }

    for child in node.children() {
        walk_code(&child, out);
    }
}
