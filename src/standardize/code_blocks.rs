//! Normalize syntax-highlighted code blocks to canonical
//! `<pre><code class="language-X">…</code></pre>`.
//!
//! Trek's markdown layer already understands canonical pre+code; this pass
//! reduces every common highlighter shape (Chroma, Shiki, ChatGPT/CodeMirror,
//! rehype-pretty-code, hljs, Prism, Pygments, Rouge, expressive-code,
//! react-syntax-highlighter, hexo, mintlify, stripe, etc.) to that canonical
//! shape so the markdown renderer sees a single representation.

use kuchikiki::NodeRef;

use crate::dom::walk::{
    descendants_post_order, get_attr, is_any_tag, new_html_element, tag_name, text_content,
};
use crate::dom::{DomCtx, DomPass};

pub struct CodeBlocks;

/// Detect a language from a class attribute. Recognises `language-X`,
/// `lang-X`, `highlight-X`, `language-text`, and chroma's
/// `chroma language-X`.
fn detect_lang_from_class(class: &str) -> Option<String> {
    let tokens: Vec<&str> = class.split_whitespace().collect();
    for tok in &tokens {
        if let Some(rest) = tok.strip_prefix("language-") {
            if !rest.is_empty() && rest != "none" && rest != "plaintext" && rest != "text" {
                return Some(rest.to_string());
            }
        }
        if let Some(rest) = tok.strip_prefix("lang-") {
            if !rest.is_empty() && rest != "none" && rest != "plaintext" && rest != "text" {
                return Some(rest.to_string());
            }
        }
        if let Some(rest) = tok.strip_prefix("highlight-source-") {
            if !rest.is_empty() {
                return Some(rest.to_string());
            }
        }
    }
    // Verso/Lean shape: `hl <lang> block`.
    if tokens.iter().any(|t| *t == "hl") && tokens.iter().any(|t| *t == "block") {
        for t in &tokens {
            if *t == "hl" || *t == "block" || *t == "token" {
                continue;
            }
            if t.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '#') {
                return Some((*t).to_string());
            }
        }
    }
    None
}

/// Find the most specific language hint among descendants.
/// Prefer `<code>` element's data-language/class, fall back to outer.
fn first_descendant_lang(node: &NodeRef) -> Option<String> {
    // First check a direct/descendant <code> for data-language attrs.
    for d in node.descendants() {
        if !is_any_tag(&d, &["code"]) {
            continue;
        }
        if let Some(lang) = get_attr(&d, "data-language") {
            if !lang.is_empty() {
                return Some(lang);
            }
        }
        if let Some(lang) = get_attr(&d, "data-lang") {
            if !lang.is_empty() {
                return Some(lang);
            }
        }
        if let Some(lang) = get_attr(&d, "language") {
            if !lang.is_empty() && lang != "none" && lang != "plaintext" && lang != "text" {
                return Some(lang);
            }
        }
        if let Some(class) = get_attr(&d, "class") {
            if let Some(l) = detect_lang_from_class(&class) {
                return Some(l);
            }
        }
    }
    // Then outer node.
    if let Some(lang) = get_attr(node, "data-language") {
        if !lang.is_empty() {
            return Some(lang);
        }
    }
    if let Some(lang) = get_attr(node, "data-lang") {
        if !lang.is_empty() {
            return Some(lang);
        }
    }
    if let Some(lang) = get_attr(node, "language") {
        if !lang.is_empty() && lang != "none" && lang != "plaintext" && lang != "text" {
            return Some(lang);
        }
    }
    if let Some(class) = get_attr(node, "class") {
        if let Some(l) = detect_lang_from_class(&class) {
            return Some(l);
        }
    }
    // Fallback: any descendant with class language-X.
    for d in node.descendants() {
        if let Some(class) = get_attr(&d, "class") {
            if let Some(l) = detect_lang_from_class(&class) {
                return Some(l);
            }
        }
    }
    None
}

/// True if `node` lives in the immediate vicinity of a `<pre>` element —
/// either the parent or grandparent contains one as a descendant other than
/// `node` itself.
fn nearby_code_block(node: &NodeRef) -> bool {
    let mut cur = node.parent();
    let mut hops = 0;
    while let Some(p) = cur {
        hops += 1;
        if hops > 3 {
            break;
        }
        for d in p.descendants() {
            if std::rc::Rc::ptr_eq(&d.0, &node.0) {
                continue;
            }
            if is_any_tag(&d, &["pre"]) {
                return true;
            }
        }
        cur = p.parent();
    }
    false
}

/// Walk up parents looking for a class/data-language that names the language.
fn ancestor_lang(node: &NodeRef) -> Option<String> {
    let mut cur = node.parent();
    let mut hops = 0;
    while let Some(p) = cur {
        hops += 1;
        if hops > 6 {
            break;
        }
        if let Some(lang) = get_attr(&p, "data-language") {
            if !lang.is_empty() {
                return Some(lang);
            }
        }
        if let Some(lang) = get_attr(&p, "data-lang") {
            if !lang.is_empty() {
                return Some(lang);
            }
        }
        if let Some(class) = get_attr(&p, "class") {
            if let Some(l) = detect_lang_from_class(&class) {
                return Some(l);
            }
        }
        if let Some(lang) = get_attr(&p, "lang") {
            if !lang.is_empty()
                && lang.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '#')
            {
                return Some(lang);
            }
        }
        cur = p.parent();
    }
    None
}

/// Build a canonical pre+code element from raw text + optional language.
fn build_canonical_pre(text: &str, lang: Option<&str>) -> NodeRef {
    let pre = new_html_element("pre", Vec::new());
    let lang_class;
    let attrs = if let Some(l) = lang {
        lang_class = format!("language-{l}");
        crate::dom::walk::build_attrs(&[("class", lang_class.as_str())])
    } else {
        Vec::new()
    };
    let code = new_html_element("code", attrs);
    code.append(NodeRef::new_text(text));
    pre.append(code);
    pre
}

/// True if a class attribute string indicates a line-number gutter.
fn is_lineno_class(class: &str) -> bool {
    let lc = class.to_ascii_lowercase();
    // Specific tokens that mean "line number column".
    for tok in lc.split_whitespace() {
        match tok {
            "lineno" | "linenos" | "line-numbers" | "line-number" | "linenumber"
            | "react-syntax-highlighter-line-number" | "ln" | "lnt" | "rouge-gutter"
            | "code-line-numbers" | "code-block-line-numbers" => {
                return true;
            }
            _ => {}
        }
        if tok == "gutter" {
            return true;
        }
        if tok.starts_with("cm-gutter") || tok == "cm-gutters" {
            return true;
        }
        if tok == "codemirror-linenumber" || tok == "codemirror-gutters" {
            return true;
        }
    }
    false
}

/// True if a class attribute string indicates code-block chrome (header,
/// copy button, language label, run button, etc.) that should be stripped
/// from extracted code text.
fn is_chrome_class(class: &str) -> bool {
    let lc = class.to_ascii_lowercase();
    for tok in lc.split_whitespace() {
        match tok {
            "code__header"
            | "code-header"
            | "codeheader"
            | "hljs-header"
            | "code-block-header"
            | "code__copy-button"
            | "code-copy-button"
            | "copy-button"
            | "copy_button"
            | "copybutton"
            | "code-copy"
            | "rehype-pretty-copy"
            | "language-label"
            | "code-toolbar"
            | "code__toolbar"
            | "filename"
            | "ec-meta"
            | "expressive-code__header"
            | "code-block__header"
            | "shiki-twoslash__header" => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// True if the class indicates a "line container" for a line of code.
fn is_line_container_class(class: &str) -> bool {
    let lc = class.to_ascii_lowercase();
    for tok in lc.split_whitespace() {
        match tok {
            "line"
            | "cm-line"
            | "ec-line"
            | "code-line"
            | "hljs-line"
            | "react-syntax-highlighter-line"
            | "highlight-line"
            | "react-code-line"
            | "v-line" => {
                return true;
            }
            _ => {}
        }
    }
    false
}

/// True if the node is a `<span>` whose only text is a small ASCII number,
/// styled as a line marker (sticky-left, no-wrap, etc.). Used to strip
/// chroma's inline line-number spans.
fn looks_like_inline_lineno(node: &NodeRef) -> bool {
    if !is_any_tag(node, &["span"]) {
        return false;
    }
    // Must be the leftmost element child (skipping whitespace text).
    let mut prev = node.previous_sibling();
    while let Some(p) = prev {
        if let Some(t) = p.as_text() {
            if t.borrow().chars().all(char::is_whitespace) {
                prev = p.previous_sibling();
                continue;
            }
            return false;
        }
        return false;
    }
    let text = text_content(node);
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.len() > 4 {
        return false;
    }
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    // Style hint: sticky select-none / user-select:none typical of line markers.
    let style = get_attr(node, "style").unwrap_or_default().to_ascii_lowercase();
    let has_style_hint = style.contains("user-select:none")
        || style.contains("user-select: none")
        || style.contains("white-space:pre");
    let class_attr = get_attr(node, "class").unwrap_or_default();
    let has_class_hint =
        is_lineno_class(&class_attr) || class_attr.to_ascii_lowercase().contains("ln");
    // Structural hint: parent looks like a flex-row gutter container, with
    // at least one element sibling that holds the code text.
    let parent_class = node
        .parent()
        .and_then(|p| get_attr(&p, "class"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    let has_parent_hint = parent_class.split_whitespace().any(|t| {
        t == "flex-row" || t == "ec-line" || t == "line" || t == "cl"
    }) && node.next_sibling().is_some();
    has_style_hint || has_class_hint || has_parent_hint
}

/// Extract canonical text from a highlighted block — concatenates descendant
/// text, preserving newlines from `<br>` and line container breaks.
fn extract_text_with_lines(node: &NodeRef) -> String {
    let mut out = String::new();
    // `last_was_line_term` tracks whether the most recent emitted child
    // element terminated its own line (a line container produced "...\n",
    // or a previous <br> already wrote a newline). Used to decide whether a
    // following <br> should add another newline.
    fn visit(n: &NodeRef, out: &mut String, depth: usize, last_was_line_term: &mut bool) {
        for c in n.children() {
            if let Some(el) = c.as_element() {
                let local_owned = el.name.local.to_string();
                let local: &str = &local_owned;
                let local_lc = local.to_ascii_lowercase();

                // <br> → newline. If the previous sibling was a line
                // container that already terminated with \n, skip; this
                // avoids doubling between lines while still allowing
                // <br><br> sequences (where the empty line is real) to
                // produce blank lines.
                if local_lc == "br" {
                    if !*last_was_line_term {
                        out.push('\n');
                    }
                    *last_was_line_term = true;
                    continue;
                }
                // Skip non-content nodes entirely.
                if matches!(local_lc.as_str(), "button" | "style" | "script" | "svg") {
                    continue;
                }
                // Skip floating-buttons / fade overlays / decorative copy
                // tooltips that some highlighters bake into the DOM.
                {
                    let attrs_borrow = el.attributes.borrow();
                    if attrs_borrow.get("data-floating-buttons").is_some()
                        || attrs_borrow.get("data-fade-overlay").is_some()
                        || attrs_borrow.get("data-copy-button").is_some()
                    {
                        continue;
                    }
                    // aria-hidden divs that are pure decoration (the tooltip
                    // bubble in mintlify/Sanity-style code blocks).
                    if attrs_borrow
                        .get("aria-hidden")
                        .map(|v| v == "true")
                        .unwrap_or(false)
                        && matches!(local_lc.as_str(), "div" | "span")
                    {
                        continue;
                    }
                }

                let class_attr = el
                    .attributes
                    .borrow()
                    .get("class")
                    .map(|s| s.to_string())
                    .unwrap_or_default();

                // Skip line-number gutters / decorations. A stripped
                // line-number marker resets `last_was_line_term` so that
                // any literal `\n` following it (between line N and N+1)
                // is preserved — important when an empty source line is
                // represented only by adjacent linenumber spans.
                if is_lineno_class(&class_attr) {
                    *last_was_line_term = false;
                    continue;
                }
                // Skip header / copy / language label chrome.
                if is_chrome_class(&class_attr) {
                    continue;
                }
                // Skip inline line-number span at the start of a line.
                if looks_like_inline_lineno(&c) {
                    continue;
                }
                // Aria-hidden line-number divs (expressive-code .ln).
                let aria_hidden = el
                    .attributes
                    .borrow()
                    .get("aria-hidden")
                    .map(|s| s.to_string())
                    .unwrap_or_default();
                if aria_hidden == "true" && (text_content(&c).trim().chars().all(|x| x.is_ascii_digit())) {
                    continue;
                }

                let is_line = is_line_container_class(&class_attr);
                let block_like = matches!(local_lc.as_str(), "p" | "div" | "li" | "tr") || is_line;

                let before_len = out.len();
                let mut child_lwlt = false;
                visit(&c, out, depth + 1, &mut child_lwlt);
                let produced = out.len() > before_len;

                if (block_like || is_line) && produced && !out.ends_with('\n') {
                    // Non-empty block produced content without a trailing
                    // newline → terminate it.
                    out.push('\n');
                    *last_was_line_term = true;
                } else if (block_like || is_line) && produced && out.ends_with('\n') {
                    *last_was_line_term = true;
                } else if is_line && !produced {
                    // An empty `<span class="line"></span>` represents a
                    // blank line; reset the flag so the next <br> emits an
                    // additional newline (matching the rendered HTML).
                    *last_was_line_term = false;
                } else if produced {
                    *last_was_line_term = out.ends_with('\n');
                }
                // else: no children produced and not a line container → leave state alone.
            } else if let Some(t) = c.as_text() {
                let s = t.borrow();
                if s.is_empty() {
                    continue;
                }
                // If we just terminated a line and this text is *only*
                // newlines/horizontal-whitespace surrounding a newline (i.e.
                // formatting between sibling line containers), skip it.
                // Real indentation like `    ` on the next code line is left
                // intact (no embedded newline).
                if *last_was_line_term && s.contains('\n') && s.chars().all(char::is_whitespace) {
                    continue;
                }
                out.push_str(&s);
                *last_was_line_term = s.ends_with('\n');
            }
        }
    }
    let mut state = false;
    visit(node, &mut out, 0, &mut state);
    out
}

fn looks_like_chroma_or_highlight(node: &NodeRef) -> bool {
    if let Some(class) = get_attr(node, "class") {
        let lc = class.to_ascii_lowercase();
        for tok in lc.split_whitespace() {
            if matches!(
                tok,
                "chroma" | "highlight" | "expressive-code" | "code-block" | "codeblock"
            ) {
                return true;
            }
            if tok.starts_with("language-") || tok.starts_with("highlight-source-") {
                return true;
            }
        }
    }
    false
}

fn looks_like_codemirror(node: &NodeRef) -> bool {
    if let Some(class) = get_attr(node, "class") {
        let lc = class.to_ascii_lowercase();
        for tok in lc.split_whitespace() {
            if tok == "cm-editor" || tok == "codemirror" || tok == "cm-content" {
                return true;
            }
        }
    }
    false
}

fn looks_like_hexo_figure(node: &NodeRef) -> bool {
    if !is_any_tag(node, &["figure"]) {
        return false;
    }
    if let Some(class) = get_attr(node, "class") {
        let lc = class.to_ascii_lowercase();
        return lc.split_whitespace().any(|t| t == "highlight");
    }
    false
}

/// Detect language hint from class like `highlight cpp` (hexo).
fn lang_from_hexo_figure_class(node: &NodeRef) -> Option<String> {
    let class = get_attr(node, "class")?;
    let mut tokens: Vec<&str> = class.split_whitespace().collect();
    // Drop "highlight" token; second token (if alphanumeric) is the lang.
    tokens.retain(|t| *t != "highlight");
    let cand = tokens.first()?;
    if cand.is_empty() || cand.chars().any(|c| !c.is_ascii_alphanumeric() && c != '+' && c != '-' && c != '#') {
        return None;
    }
    if matches!(*cand, "plaintext" | "text" | "none") {
        return None;
    }
    Some((*cand).to_string())
}

impl DomPass for CodeBlocks {
    fn name(&self) -> &'static str {
        "code_blocks"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        // Track-D's `elements::normalize_all` (src/elements/code.rs) already
        // handles Chroma/Pygments/Shiki/CodeMirror normalisation downstream
        // of this pass. We restrict ourselves to Pass A — wrapping bare
        // `<code style="white-space:pre">` in a `<pre>` so Track-D sees a
        // canonical block container.
        let _ = build_canonical_pre;
        let _ = extract_text_with_lines;
        let _ = looks_like_chroma_or_highlight;
        let _ = looks_like_codemirror;
        let _ = first_descendant_lang;
        let _ = trim_lines;
        let _ = text_content;
        // Pass A: <code style="white-space:pre"> not inside a <pre> → wrap in <pre>.
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["code"]) {
                continue;
            }
            // Inside <pre> already?
            let inside_pre = {
                let mut cur = node.parent();
                let mut found = false;
                while let Some(p) = cur {
                    if is_any_tag(&p, &["pre"]) {
                        found = true;
                        break;
                    }
                    cur = p.parent();
                }
                found
            };
            if inside_pre {
                continue;
            }
            let style = get_attr(&node, "style").unwrap_or_default().to_ascii_lowercase();
            let class = get_attr(&node, "class").unwrap_or_default().to_ascii_lowercase();
            // Promote to <pre> if it has white-space:pre, OR a `block` class
            // (lean-verso `code.hl.block`), OR `display: block` style.
            let is_block_code = class.split_whitespace().any(|t| t == "block")
                || style.contains("display:block")
                || style.replace(' ', "").contains("white-space:pre");
            if is_block_code {
                let pre = new_html_element("pre", Vec::new());
                node.insert_before(pre.clone());
                pre.append(node);
            }
        }

        // Pass A2: hexo `<figure class="highlight LANG">` → canonical pre.
        let hexo_figs: Vec<NodeRef> = root
            .descendants()
            .filter(|d| looks_like_hexo_figure(d))
            .collect();
        for fig in hexo_figs {
            if fig.parent().is_none() {
                continue;
            }
            // The structure has table > tr > td.gutter (skip) + td.code (keep).
            // Pull text from td.code only.
            let mut text = String::new();
            for td in fig.descendants() {
                if !is_any_tag(&td, &["td"]) {
                    continue;
                }
                let cls = get_attr(&td, "class").unwrap_or_default();
                if cls.split_whitespace().any(|t| t == "code") {
                    text = extract_text_with_lines(&td);
                    break;
                }
            }
            if text.is_empty() {
                text = extract_text_with_lines(&fig);
            }
            let lang = lang_from_hexo_figure_class(&fig);
            let cleaned = trim_lines(&text);
            if cleaned.trim().is_empty() {
                continue;
            }
            let canonical = build_canonical_pre(&cleaned, lang.as_deref());
            fig.insert_before(canonical);
            fig.detach();
        }

        // Pass A3: preceding-sibling clean-up. For every <pre> in the
        // tree, walk up a few parents and detach short text labels /
        // language-name spans that appear *before* the pre as siblings of
        // the pre or of an ancestor wrapper. This removes the "java"-style
        // label headers from react-syntax-highlighter and similar wrappers
        // even after legacy flatten has dissolved their original container.
        let pres: Vec<NodeRef> = root
            .descendants()
            .filter(|d| is_any_tag(d, &["pre"]))
            .collect();
        for pre in pres {
            // Collect candidate sibling elements whose role is plausibly
            // "code-block label" — an element preceding the pre at any
            // ancestor level, with a tiny single-token text body.
            let mut victims: Vec<NodeRef> = Vec::new();
            let mut anchor = pre.clone();
            let mut hops = 0;
            while hops < 4 {
                hops += 1;
                let parent = match anchor.parent() {
                    Some(p) => p,
                    None => break,
                };
                let mut sib = anchor.previous_sibling();
                while let Some(s) = sib {
                    if let Some(el) = s.as_element() {
                        let local: &str = &el.name.local;
                        if local.eq_ignore_ascii_case("pre")
                            || local.eq_ignore_ascii_case("p")
                            || local.eq_ignore_ascii_case("h1")
                            || local.eq_ignore_ascii_case("h2")
                            || local.eq_ignore_ascii_case("h3")
                            || local.eq_ignore_ascii_case("h4")
                            || local.eq_ignore_ascii_case("h5")
                            || local.eq_ignore_ascii_case("h6")
                            || local.eq_ignore_ascii_case("ul")
                            || local.eq_ignore_ascii_case("ol")
                            || local.eq_ignore_ascii_case("table")
                            || local.eq_ignore_ascii_case("blockquote")
                        {
                            break;
                        }
                        if local.eq_ignore_ascii_case("button") {
                            victims.push(s.clone());
                            sib = s.previous_sibling();
                            continue;
                        }
                        if local.eq_ignore_ascii_case("span") || local.eq_ignore_ascii_case("div") {
                            let txt = text_content(&s);
                            let trimmed = txt.trim();
                            if trimmed.is_empty()
                                || (trimmed.len() <= 16
                                    && !trimmed.contains(char::is_whitespace)
                                    && trimmed
                                        .chars()
                                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '#')))
                            {
                                victims.push(s.clone());
                                sib = s.previous_sibling();
                                continue;
                            }
                            // Very short text like "Copy" / "Run".
                            if matches!(trimmed.to_ascii_lowercase().as_str(), "copy" | "run" | "copy code") {
                                victims.push(s.clone());
                                sib = s.previous_sibling();
                                continue;
                            }
                        }
                    }
                    break;
                }
                anchor = parent;
            }
            for v in victims {
                if v.parent().is_some() {
                    v.detach();
                }
            }
        }

        // Pass B: replace highlighter shapes with canonical pre+code.
        let candidates: Vec<NodeRef> = root
            .descendants()
            .filter(|d| {
                if let Some(name) = tag_name(d) {
                    if name == "pre" {
                        return true;
                    }
                    if name == "div"
                        && (looks_like_chroma_or_highlight(d) || looks_like_codemirror(d))
                    {
                        return true;
                    }
                }
                false
            })
            .collect();

        for cand in candidates {
            if cand.parent().is_none() {
                continue;
            }
            let name = tag_name(&cand).unwrap_or_default();
            // If this candidate IS a line-number gutter pre (e.g.
            // `<pre class="lineno">`), drop it entirely.
            if name == "pre" {
                if let Some(c) = get_attr(&cand, "class") {
                    if is_lineno_class(&c) {
                        cand.detach();
                        continue;
                    }
                }
            }

            // For <pre>, normalize when there's any nested decoration the
            // markdown layer would otherwise render verbatim.
            let needs_normalize = if name == "pre" {
                let has_table = cand.descendants().any(|d| is_any_tag(&d, &["table"]));
                let has_lineno = cand.descendants().any(|d| {
                    if let Some(c) = get_attr(&d, "class") {
                        return is_lineno_class(&c);
                    }
                    false
                });
                let has_chrome = cand.descendants().any(|d| {
                    if let Some(c) = get_attr(&d, "class") {
                        return is_chrome_class(&c);
                    }
                    false
                });
                let has_line_container = cand.descendants().any(|d| {
                    if let Some(c) = get_attr(&d, "class") {
                        return is_line_container_class(&c);
                    }
                    false
                });
                let has_chroma_lines = cand.descendants().any(|d| {
                    if let Some(c) = get_attr(&d, "class") {
                        let lc = c.to_ascii_lowercase();
                        return lc.contains("chroma") && (lc.contains("line") || lc.contains("ln"));
                    }
                    false
                });
                let has_br_in_code = cand
                    .descendants()
                    .any(|d| is_any_tag(&d, &["br"]));
                let has_button = cand.descendants().any(|d| is_any_tag(&d, &["button"]));
                let has_inline_lineno = cand.descendants().any(|d| looks_like_inline_lineno(&d));
                let has_div_children = cand
                    .children()
                    .any(|c| is_any_tag(&c, &["div"]))
                    || cand.descendants().any(|d| {
                        if !is_any_tag(&d, &["code"]) {
                            return false;
                        }
                        d.children().any(|c| is_any_tag(&c, &["div"]))
                    });
                has_table
                    || has_lineno
                    || has_chrome
                    || has_line_container
                    || has_chroma_lines
                    || has_br_in_code
                    || has_button
                    || has_inline_lineno
                    || has_div_children
            } else {
                true
            };

            if !needs_normalize {
                continue;
            }

            let lang = first_descendant_lang(&cand).or_else(|| ancestor_lang(&cand));
            let raw_text = extract_text_with_lines(&cand);
            let cleaned = trim_lines(&raw_text);
            // Skip if there's nothing meaningful left.
            if cleaned.trim().is_empty() {
                continue;
            }
            let canonical = build_canonical_pre(&cleaned, lang.as_deref());
            cand.insert_before(canonical);
            cand.detach();
        }

        // Pass B2: detach copy buttons / language label spans that sit
        // adjacent to a `<pre>` (typical of react-syntax-highlighter and
        // similar wrappers where the chrome is a sibling of the code).
        let buttons: Vec<NodeRef> = root
            .descendants()
            .filter(|d| is_any_tag(d, &["button"]))
            .collect();
        for b in buttons {
            // Drop the button if it lives near a <pre> (sibling, parent's
            // sibling, etc.) — a code-block toolbar button.
            let near_code = nearby_code_block(&b);
            if near_code {
                b.detach();
            }
        }
        // Spans whose only purpose is a language label sitting next to the
        // pre. Heuristic: small text-content (≤ 16 chars), single word,
        // located in a wrapper that contains a <pre> sibling.
        let lang_label_spans: Vec<NodeRef> = root
            .descendants()
            .filter(|d| is_any_tag(d, &["span"]))
            .collect();
        for s in lang_label_spans {
            let text = text_content(&s);
            let trimmed = text.trim();
            if trimmed.is_empty() || trimmed.len() > 16 || trimmed.contains(char::is_whitespace) {
                continue;
            }
            if !trimmed.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '#' || c == '-') {
                continue;
            }
            // Must be immediate child of a wrapper that also contains a <pre>.
            let parent = match s.parent() {
                Some(p) => p,
                None => continue,
            };
            // Skip if span sits inside the <pre> itself.
            if parent.descendants().any(|d| is_any_tag(&d, &["pre"]))
                && !is_any_tag(&parent, &["pre"])
            {
                // Also avoid dropping actual paragraph language labels.
                // Only drop spans that are the FIRST child of their parent,
                // to bias toward "header label" placements.
                let mut prev = s.previous_sibling();
                let mut leftmost = true;
                while let Some(p) = prev.take() {
                    if let Some(t) = p.as_text() {
                        if t.borrow().chars().all(char::is_whitespace) {
                            prev = p.previous_sibling();
                            continue;
                        }
                    }
                    leftmost = false;
                    break;
                }
                if leftmost {
                    s.detach();
                }
            }
        }

        // Pass C: drop hljs header chrome (`.hljs-header`, `.copy-button`, etc.)
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["div", "button", "span"]) {
                continue;
            }
            if let Some(class) = get_attr(&node, "class") {
                let lc = class.to_ascii_lowercase();
                if (lc.contains("hljs") && (lc.contains("header") || lc.contains("toolbar")))
                    || lc.contains("copy-button")
                    || lc.contains("code-toolbar")
                {
                    // Only remove when inside / adjacent to a code block.
                    let inside_code_context = {
                        let mut cur = node.parent();
                        let mut found = false;
                        while let Some(p) = cur {
                            if is_any_tag(&p, &["pre", "figure"]) {
                                found = true;
                                break;
                            }
                            if let Some(c) = get_attr(&p, "class") {
                                let lc2 = c.to_ascii_lowercase();
                                if lc2.contains("code") || lc2.contains("highlight") {
                                    found = true;
                                    break;
                                }
                            }
                            cur = p.parent();
                        }
                        found
                    };
                    if inside_code_context {
                        node.detach();
                    }
                }
            }
        }

        // Pass C1: for any `<pre><code class="block ...">` lean-verso-style
        // shape, force normalize so leading/trailing blank lines and
        // syntax-highlight spans are collapsed.
        let block_codes: Vec<NodeRef> = root
            .descendants()
            .filter(|d| {
                if !is_any_tag(d, &["code"]) {
                    return false;
                }
                let class = get_attr(d, "class").unwrap_or_default();
                class.split_whitespace().any(|t| t == "block")
                    && d.parent().map(|p| is_any_tag(&p, &["pre"])).unwrap_or(false)
            })
            .collect();
        for code in block_codes {
            let pre = match code.parent() {
                Some(p) => p,
                None => continue,
            };
            if pre.parent().is_none() {
                continue;
            }
            let raw_text = extract_text_with_lines(&code);
            let cleaned = trim_lines(&dedent(&raw_text));
            if cleaned.trim().is_empty() {
                continue;
            }
            let lang = first_descendant_lang(&code).or_else(|| ancestor_lang(&pre));
            let canonical = build_canonical_pre(&cleaned, lang.as_deref());
            pre.insert_before(canonical);
            pre.detach();
        }

        // Pass C2: rewrite non-canonical language classes on inner <code>
        // elements (e.g. `hl lean block` → `language-lean`) so the markdown
        // layer's regex picks them up.
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["code"]) {
                continue;
            }
            let class = match get_attr(&node, "class") {
                Some(c) => c,
                None => continue,
            };
            // If already has language-X, leave alone.
            if class
                .split_whitespace()
                .any(|t| t.starts_with("language-") || t.starts_with("lang-"))
            {
                continue;
            }
            if let Some(lang) = detect_lang_from_class(&class) {
                let new_class = format!("language-{lang}");
                if let Some(el) = node.as_element() {
                    el.attributes.borrow_mut().insert("class", new_class);
                }
                continue;
            }
            // Try data-language attributes.
            if let Some(lang) = get_attr(&node, "data-language") {
                if !lang.is_empty() && lang != "none" && lang != "plaintext" && lang != "text" {
                    let new_class = format!("language-{lang}");
                    if let Some(el) = node.as_element() {
                        el.attributes.borrow_mut().insert("class", new_class);
                    }
                }
            }
        }

        // Pass D: ensure every <pre> contains a single <code> child (move text
        // children into a <code> element). This makes downstream markdown
        // formatting consistent.
        for node in descendants_post_order(root) {
            if !is_any_tag(&node, &["pre"]) {
                continue;
            }
            let kids: Vec<NodeRef> = node.children().collect();
            let has_code_child = kids.iter().any(|k| is_any_tag(k, &["code"]));
            if has_code_child {
                continue;
            }
            // Wrap children inside a <code>.
            let text = text_content(&node);
            if text.trim().is_empty() {
                continue;
            }
            let lang = first_descendant_lang(&node);
            // Clear children.
            for k in &kids {
                k.detach();
            }
            let lang_class;
            let code_attrs = if let Some(l) = lang {
                lang_class = format!("language-{l}");
                crate::dom::walk::build_attrs(&[("class", lang_class.as_str())])
            } else {
                Vec::new()
            };
            let code = new_html_element("code", code_attrs);
            code.append(NodeRef::new_text(text));
            node.append(code);
        }
    }
}

/// Strip common leading whitespace from every non-empty line.
/// Useful when an inline code-block container preserved the source-level
/// indentation that HTML happened to inherit from its surrounding markup.
fn dedent(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let common = lines
        .iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(|c: char| c == ' ' || c == '\t').len())
        .min()
        .unwrap_or(0);
    if common == 0 {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        if line.len() >= common {
            out.push_str(&line[common..]);
        } else {
            out.push_str(line);
        }
    }
    out
}

/// Strip empty leading/trailing lines and collapse 3+ consecutive newlines.
fn trim_lines(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().collect();
    while let Some(last) = lines.last() {
        if last.trim().is_empty() {
            lines.pop();
        } else {
            break;
        }
    }
    let mut start = 0;
    while start < lines.len() && lines[start].trim().is_empty() {
        start += 1;
    }
    let kept: Vec<&str> = lines[start..].to_vec();
    let mut out = String::new();
    let mut prev_blank = 0usize;
    for line in kept {
        if line.trim().is_empty() {
            prev_blank += 1;
            if prev_blank > 1 {
                continue;
            }
        } else {
            prev_blank = 0;
        }
        out.push_str(line);
        out.push('\n');
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}
