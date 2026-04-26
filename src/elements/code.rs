//! Code block normalization (Track D).
//!
//! Defuddle ports `code.ts` here, but trimmed to the cases required for the
//! body-fixture pass-rate to climb. The big-ticket cases:
//!
//! 1. Detect language from `class` (`language-X`/`lang-X`),
//!    `data-language`/`data-lang`, an enclosing `<header>` text, or a parent
//!    figure annotation.
//! 2. Strip Chroma `<table class="lntable">` line-number gutters → keep the
//!    code column.
//! 3. Strip Pygments `<td class="lineno">` cells.
//! 4. Strip ChatGPT CodeMirror `.cm-gutter` columns; coalesce `.cm-line` /
//!    `[data-line]` (Shiki/rehype-pretty-code) lines.
//! 5. Strip `button.copy`, `[aria-label="Copy"]`.
//! 6. Output canonical `<pre><code class="language-X">…</code></pre>`.

use kuchikiki::NodeRef;
use kuchikiki::iter::NodeIterator;

use super::util::{
    attr, class_list, descendants_elements, has_class, new_element, remove_attr, select_all,
    select_first, set_attr, transfer_children,
};

/// Normalize `<pre>` blocks under `root`.
pub fn normalize_code_blocks(root: &NodeRef) {
    // Strip copy buttons globally first.
    strip_copy_buttons(root);

    // Strip Chroma `<table class="lntable">` line-number tables wherever
    // they live — must run before `<pre>` candidate collection so the
    // surviving `<pre>` is at the top level rather than buried inside
    // a `<td>` that the markdown renderer would otherwise treat as a
    // table cell.
    strip_chroma_lntable(root);

    // Convert known wrappers to <pre><code>. We handle <pre> elements but
    // also div-based highlighters.
    let candidates = collect_code_candidates(root);
    for node in candidates {
        process_code_block(&node);
    }
}

fn strip_copy_buttons(root: &NodeRef) {
    // Buttons / icons explicitly marked as copy controls.
    let selectors = [
        r#"button.copy"#,
        r#"button[aria-label="Copy"]"#,
        r#"button[aria-label="Copy code"]"#,
        r#"button[class*="codeblock-button"]"#,
        r#"button[data-copy]"#,
        r#"[aria-label="Copy"]"#,
        r#"[class*="copy-button"]"#,
    ];
    for sel in selectors {
        for n in select_all(root, sel) {
            n.detach();
        }
    }
}

/// Collect `<pre>` and div-based highlighter blocks. Avoid descending into
/// elements we already converted by re-collecting on each entry.
fn collect_code_candidates(root: &NodeRef) -> Vec<NodeRef> {
    let mut out: Vec<NodeRef> = Vec::new();
    for d in root.descendants().elements() {
        let n = d.as_node().clone();
        let tag = d.name.local.to_string().to_ascii_lowercase();
        if tag == "pre" {
            out.push(n);
            continue;
        }
        // div.highlight, div.syntaxhighlighter, div.language-X
        if tag == "div" {
            let cls = class_list(&n);
            if cls.iter().any(|c| {
                c == "highlight"
                    || c == "syntaxhighlighter"
                    || c == "highlight-source"
                    || c.starts_with("language-")
                    || c == "wp-block-code"
                    || c.starts_with("prismjs")
            }) {
                out.push(n);
            }
        }
    }
    out
}

fn process_code_block(node: &NodeRef) {
    // Detect language up front; we use it as a signal of "this is a code
    // block we should rewrite into the canonical <pre><code> shape".
    let lang = detect_language(node);

    // Strip noise that's safe to remove regardless of whether we rewrite.
    // (Chroma `<table.lntable>` is handled globally in `normalize_code_blocks`
    // because the table sits *above* the `<pre>` in the tree.)
    strip_pygments_lineno(node);
    strip_cm_gutter(node);
    coalesce_line_spans(node);

    // Conservative rewrite policy: only collapse the block into a fresh
    // canonical <pre><code class="language-X"> when:
    //   * we successfully detected a language, AND
    //   * the existing block isn't already a perfectly-shaped pre><code>
    //     (otherwise we churn unnecessarily and risk losing structure the
    //     markdown renderer can already handle).
    if lang.is_empty() {
        return;
    }

    if is_canonical_pre_code(node, &lang) {
        return;
    }

    // Pull text content (guarded: use inner <code> when available).
    let text = extract_text(node);
    let cleaned = clean_code_text(&text);
    if cleaned.is_empty() {
        return;
    }

    let class_value = format!("language-{lang}");
    let code_attrs: Vec<(&str, &str)> = vec![("class", class_value.as_str())];
    let code_el = new_element("code", &code_attrs);
    code_el.append(NodeRef::new_text(&cleaned));

    let pre_el = new_element("pre", &[]);
    pre_el.append(code_el);

    node.insert_before(pre_el);
    node.detach();
}

/// True if `node` is already `<pre><code class="language-LANG">…</code></pre>`.
fn is_canonical_pre_code(node: &NodeRef, lang: &str) -> bool {
    use super::util::is_tag;
    if !is_tag(node, "pre") {
        return false;
    }
    let mut found = false;
    for child in node.children() {
        if !child.as_element().is_some() {
            continue;
        }
        if found {
            return false; // multiple element children
        }
        if !is_tag(&child, "code") {
            return false;
        }
        let cls = attr(&child, "class").unwrap_or_default();
        let target = format!("language-{lang}");
        if !cls.split_whitespace().any(|c| c == target) {
            return false;
        }
        found = true;
    }
    found
}

/// Detect language via class attributes, data-* attributes, and parent
/// `<header>` text. Returns the bare language name (e.g. `"rust"`).
pub fn detect_language(node: &NodeRef) -> String {
    if let Some(s) = lang_from_classes(node) {
        return s;
    }
    // Inner <code> element.
    if let Some(code) = select_first(node, "code") {
        if let Some(s) = lang_from_classes(&code) {
            return s;
        }
    }
    // Data attributes on the outer or any descendant <code>.
    for cand in std::iter::once(node.clone()).chain(select_all(node, "code")) {
        for key in ["data-language", "data-lang", "language"] {
            if let Some(v) = attr(&cand, key) {
                let v = v.trim().to_lowercase();
                if !v.is_empty() {
                    return v;
                }
            }
        }
    }
    // Parent or sibling header text (e.g. hljs-header).
    if let Some(parent) = node.parent() {
        for child in parent.children() {
            if !child.as_element().is_some() {
                continue;
            }
            // Don't consume the code block itself.
            if std::rc::Rc::ptr_eq(&child.0, &node.0) {
                continue;
            }
            let tag = child
                .as_element()
                .map(|e| e.name.local.to_string())
                .unwrap_or_default();
            if tag == "header"
                || has_class(&child, "hljs-header")
                || has_class(&child, "code-block-header")
                || has_class(&child, "code-header")
            {
                let txt = child.text_contents();
                let token = txt.split_whitespace().next().unwrap_or("").to_lowercase();
                if !token.is_empty()
                    && token
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '#' || c == '-')
                {
                    return token;
                }
            }
        }
    }
    String::new()
}

fn lang_from_classes(node: &NodeRef) -> Option<String> {
    let classes = class_list(node);
    for c in &classes {
        if let Some(rest) = c.strip_prefix("language-") {
            if !rest.is_empty() {
                return Some(rest.to_lowercase());
            }
        }
        if let Some(rest) = c.strip_prefix("lang-") {
            if !rest.is_empty() {
                return Some(rest.to_lowercase());
            }
        }
    }
    // hljs uses just bare class names sometimes; we're conservative and
    // skip those to avoid false positives.
    None
}

fn strip_chroma_lntable(node: &NodeRef) {
    // Chroma uses <table class="lntable"><tr><td class="lntd"><pre><code> ...
    // The gutter td has <code> whose only descendants are <span class="lnt">.
    // The code td has <code class="language-X"> with rich syntax-highlight spans.
    for table in select_all(node, "table.lntable") {
        for td in select_all(&table, "td.lntd") {
            let lnt_spans = select_all(&td, "span.lnt").len();
            let total_spans = select_all(&td, "span").len();
            // Gutter: every span inside is a `.lnt`.
            if lnt_spans > 0 && lnt_spans == total_spans {
                td.detach();
            }
        }
        // Replace the table with its remaining inner <pre>.
        if let Some(inner_pre) = select_first(&table, "pre") {
            inner_pre.detach();
            table.insert_before(inner_pre.clone());
            table.detach();
        }
    }
}

fn strip_pygments_lineno(node: &NodeRef) {
    for n in select_all(node, "td.lineno") {
        n.detach();
    }
    for n in select_all(node, "td.linenos") {
        n.detach();
    }
    // Pygments line-number anchor spans inside the code body.
    for n in select_all(node, "span.lineno") {
        n.detach();
    }
}

fn strip_cm_gutter(node: &NodeRef) {
    for n in select_all(node, ".cm-gutter, .cm-gutters") {
        n.detach();
    }
}

fn coalesce_line_spans(node: &NodeRef) {
    // Shiki / rehype-pretty-code wraps each line in `<span data-line>`.
    let lines = select_all(node, "span[data-line], .cm-line");
    if lines.is_empty() {
        return;
    }
    // Build a newline-joined text representation by grabbing each line's text.
    // We replace the parent element children with a single text node; this is
    // safe when the lines are direct siblings under one container.
    // Find a common parent (use the first match's parent).
    let parent = match lines[0].parent() {
        Some(p) => p,
        None => return,
    };
    // Verify all lines share the same parent — if not, bail.
    for l in &lines {
        if l.parent()
            .map(|p| !std::rc::Rc::ptr_eq(&p.0, &parent.0))
            .unwrap_or(true)
        {
            return;
        }
    }
    let mut joined = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            joined.push('\n');
        }
        joined.push_str(&l.text_contents());
    }
    // Detach all lines, then append the joined text.
    for l in &lines {
        l.detach();
    }
    parent.append(NodeRef::new_text(&joined));
}

/// Best-effort code text extraction. Walks descendants and concatenates text
/// nodes verbatim, preserving newlines.
pub fn extract_text(node: &NodeRef) -> String {
    // If the node has an inner `<code>`, prefer that.
    let target = select_first(node, "code").unwrap_or_else(|| node.clone());
    target.text_contents()
}

fn clean_code_text(s: &str) -> String {
    // Tabs → 4 spaces, NBSP → space, normalize 3+ newlines to 2, trim end.
    let mut out = s.replace('\t', "    ").replace('\u{00A0}', " ");
    while out.contains("\n\n\n") {
        out = out.replace("\n\n\n", "\n\n");
    }
    let trimmed = out
        .trim_end_matches(|c: char| c == '\n' || c == ' ')
        .to_string();
    trimmed
}

#[allow(dead_code)]
fn _keep_imports(_n: &NodeRef) {
    let _ = (
        descendants_elements,
        set_attr,
        remove_attr,
        transfer_children,
    );
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
    fn detects_language_class() {
        let root = parse(r#"<pre><code class="language-rust">fn x() {}</code></pre>"#);
        normalize_code_blocks(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"class="language-rust""#), "got: {out}");
        assert!(out.contains("fn x()"), "got: {out}");
    }

    #[test]
    fn detects_lang_dash_class() {
        let root = parse(r#"<pre class="lang-python"><code>print(1)</code></pre>"#);
        normalize_code_blocks(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"class="language-python""#), "got: {out}");
    }

    #[test]
    fn shiki_data_line_coalesces() {
        let html = r#"<pre><code><span data-line>line one</span><span data-line>line two</span></code></pre>"#;
        let root = parse(html);
        normalize_code_blocks(&root);
        let out = serialize(&root);
        assert!(out.contains("line one\nline two"), "got: {out}");
    }

    #[test]
    fn chroma_lntable_is_stripped() {
        let html = r##"<html><body><div class="highlight"><div class="chroma"><table class="lntable"><tbody><tr><td class="lntd"><pre class="chroma"><code><span class="lnt">1</span><span class="lnt">2</span></code></pre></td><td class="lntd"><pre class="chroma"><code class="language-cpp" data-lang="cpp">int x;</code></pre></td></tr></tbody></table></div></div></body></html>"##;
        let root = parse(html);
        normalize_code_blocks(&root);
        let out = serialize(&root);
        assert!(!out.contains("lntable"), "got: {out}");
        assert!(!out.contains("class=\"lnt\""), "got: {out}");
        assert!(out.contains("int x;"), "got: {out}");
        assert!(out.contains("language-cpp"), "got: {out}");
    }

    #[test]
    fn copy_button_is_stripped() {
        let html = r#"<pre><button class="copy">Copy</button><code>a</code></pre>"#;
        let root = parse(html);
        normalize_code_blocks(&root);
        let out = serialize(&root);
        assert!(!out.contains("Copy"), "got: {out}");
        assert!(out.contains(">a<"), "got: {out}");
    }
}
