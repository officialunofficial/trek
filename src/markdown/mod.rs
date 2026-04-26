//! HTML → Markdown conversion.
//!
//! Walks a `kuchikiki` DOM and emits Markdown closely matching Defuddle's
//! `markdown.ts` output. Used to populate `TrekResponse::content_markdown`
//! when the corresponding output flag is set.
//!
//! Public surface:
//! * [`html_to_markdown`] — parse a fragment of HTML and convert.
//! * [`node_to_markdown`] — convert an existing DOM subtree (used by
//!   site-specific extractors that already hold a parsed tree).

use kuchikiki::NodeRef;
use kuchikiki::traits::TendrilSink;
use once_cell::sync::Lazy;
use regex::Regex;

mod code;
mod escape;
mod figures;
mod links;
mod tables;
mod util;

use escape::{decode_entities, escape_md_text};
use util::{
    attr, ensure_trailing_newlines, has_any_class, has_class, is_any_tag, is_tag, tag_name,
    trim_end_newlines,
};

/// Convert an HTML fragment to Markdown.
#[must_use]
pub fn html_to_markdown(html: &str) -> String {
    html_to_markdown_with(html, "", None)
}

/// Convert HTML to Markdown, also stripping a leading heading that matches
/// the supplied article title. Mirrors Defuddle's `stripLeadingH1`.
#[must_use]
pub fn html_to_markdown_with_title(html: &str, title: &str) -> String {
    html_to_markdown_with(html, title, None)
}

/// Convert HTML to Markdown using a title to strip a duplicate leading
/// heading and a base URL to resolve relative links.
#[must_use]
pub fn html_to_markdown_with(html: &str, title: &str, base_url: Option<&str>) -> String {
    let cleaned = strip_wbr(html);
    let dom = kuchikiki::parse_html().one(cleaned.as_str());
    node_to_markdown_with(&dom, title, base_url)
}

/// Convert a DOM subtree to Markdown.
#[must_use]
pub fn node_to_markdown(node: &NodeRef) -> String {
    node_to_markdown_with(node, "", None)
}

/// Variant of [`node_to_markdown`] that strips a leading heading that
/// duplicates the article title.
#[must_use]
pub fn node_to_markdown_with_title(node: &NodeRef, title: &str) -> String {
    node_to_markdown_with(node, title, None)
}

/// Full-control variant.
#[must_use]
pub fn node_to_markdown_with(node: &NodeRef, title: &str, base_url: Option<&str>) -> String {
    let mut renderer = Renderer::new();
    renderer.base_url = base_url.map(std::string::ToString::to_string);
    let body = locate_body(node).unwrap_or_else(|| node.clone());
    let mut out = renderer.render_children(&body);
    let footnotes = std::mem::take(&mut renderer.footnotes);
    out = post_process(&out, &footnotes, title);
    out
}

/// Locate `<body>` if `node` is a full document; otherwise return `None`.
fn locate_body(node: &NodeRef) -> Option<NodeRef> {
    node.descendants().find(|n| is_tag(n, "body"))
}

fn strip_wbr(html: &str) -> String {
    static WBR_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?i)<wbr\s*/?>|</wbr>").expect("wbr regex"));
    WBR_RE.replace_all(html, "").into_owned()
}

/// Renderer state carried through the tree walk.
#[derive(Default)]
struct Renderer {
    /// Stack of (kind, ordinal_or_zero) for ancestor lists. Used to compute
    /// indentation depth and bullet/numbering.
    list_stack: Vec<ListFrame>,
    /// In-progress footnote definitions, keyed by id.
    footnotes: Vec<(String, String)>,
    /// True while rendering inside a `<table>` cell — math handling switches
    /// to inline mode regardless of element class.
    in_table: bool,
    /// True while rendering inside a `<pre>` block — text shouldn't be
    /// markdown-escaped or whitespace-normalized.
    in_pre: bool,
    /// Base URL for resolving relative `href`s and `src`s.
    base_url: Option<String>,
}

#[derive(Clone, Copy)]
struct ListFrame {
    ordered: bool,
    /// Next item index for an ordered list (1-based).
    next: u32,
}

impl Renderer {
    fn new() -> Self {
        Self::default()
    }

    /// Render the children of `node` as block-level content, joined by
    /// appropriate newlines.
    fn render_children(&mut self, node: &NodeRef) -> String {
        let mut out = String::new();
        for child in node.children() {
            self.render_block_or_inline(&child, &mut out);
        }
        out
    }

    fn render_block_or_inline(&mut self, node: &NodeRef, out: &mut String) {
        // Text nodes between blocks are typically whitespace and ignored.
        if let Some(text) = node.as_text() {
            let raw = text.borrow();
            // Inside a block context, treat all-whitespace text nodes as a
            // separator. Non-whitespace text gets wrapped in a synthetic
            // paragraph-like flush.
            if raw.trim().is_empty() {
                return;
            }
            // Stray inline text — wrap as a paragraph so it doesn't merge
            // into the previous block.
            if !out.is_empty() && !out.ends_with("\n\n") {
                ensure_trailing_newlines(out, 2);
            }
            out.push_str(&self.render_inline_text(&raw));
            return;
        }

        let Some(_) = node.as_element() else {
            // Comments, doctypes, etc.
            return;
        };

        let tag = tag_name(node);
        match tag.as_str() {
            "script" | "style" | "noscript" | "template" => {}
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => self.render_heading(node, &tag, out),
            "p" => self.render_paragraph(node, out),
            "br" => {
                // Stray <br> at block level → a blank line.
                ensure_trailing_newlines(out, 2);
            }
            "hr" => {
                ensure_trailing_newlines(out, 2);
                out.push_str("---");
                ensure_trailing_newlines(out, 2);
            }
            "blockquote" => self.render_blockquote(node, out),
            "ul" | "ol" => self.render_list(node, out),
            "li" => {
                // Stray <li> outside of a list — render its children as a paragraph.
                self.render_paragraph(node, out);
            }
            "pre" => self.render_pre(node, out),
            "table" => self.render_table(node, out),
            "figure" => self.render_figure(node, out),
            "figcaption" => self.render_paragraph(node, out),
            "dl" => self.render_dl(node, out),
            "details" => self.render_paragraph(node, out),
            "div" | "section" | "article" | "main" | "aside" | "header" | "footer" | "nav" => {
                self.render_div_like(node, out);
            }
            // Inline-ish elements that some pages use as block wrappers.
            "code" => {
                // Outer <code> wrapping a <pre> means the outer is just a
                // highlight-container — defer to the inner pre.
                if node.descendants().any(|n| is_tag(&n, "pre")) {
                    for child in node.children() {
                        self.render_block_or_inline(&child, out);
                    }
                } else {
                    let inline = self.render_inline(node);
                    if !inline.trim().is_empty() {
                        if !out.is_empty() && !out.ends_with("\n\n") {
                            ensure_trailing_newlines(out, 2);
                        }
                        out.push_str(&inline);
                        ensure_trailing_newlines(out, 2);
                    }
                }
            }
            "span" | "a" | "em" | "i" | "strong" | "b" | "mark" | "sub" | "sup" => {
                let mut buf = String::new();
                self.render_inline_node(node, &mut buf);
                if !buf.trim().is_empty() {
                    if !out.is_empty() && !out.ends_with("\n\n") {
                        ensure_trailing_newlines(out, 2);
                    }
                    out.push_str(&buf);
                    ensure_trailing_newlines(out, 2);
                }
            }
            "img" => {
                let img = self.render_image(node);
                if !img.is_empty() {
                    ensure_trailing_newlines(out, 2);
                    out.push_str(&img);
                    ensure_trailing_newlines(out, 2);
                }
            }
            "iframe" | "video" | "audio" => {
                // Pass through as raw HTML.
                let raw = serialize_node(node);
                if !raw.trim().is_empty() {
                    ensure_trailing_newlines(out, 2);
                    out.push_str(raw.trim());
                    ensure_trailing_newlines(out, 2);
                }
            }
            "math" | "svg" => {
                if let Some(latex) = mathml_latex(node) {
                    ensure_trailing_newlines(out, 2);
                    out.push_str("$$\n");
                    out.push_str(&latex);
                    out.push_str("\n$$");
                    ensure_trailing_newlines(out, 2);
                }
            }
            _ => self.render_div_like(node, out),
        }
    }

    fn render_div_like(&mut self, node: &NodeRef, out: &mut String) {
        // Detect Obsidian-style callout blockquote replacement.
        if is_callout(node) {
            self.render_callout(node, out);
            return;
        }
        // Pulldown-cmark style footnote definition.
        if has_class(node, "footnote-definition") {
            let id = attr(node, "id").unwrap_or_default();
            let id = id.split('-').next().unwrap_or(&id).to_string();
            let mut buf = String::new();
            for child in node.children() {
                // Skip the definition label (e.g. <sup class="footnote-definition-label">).
                if has_class(&child, "footnote-definition-label") {
                    continue;
                }
                self.render_block_or_inline(&child, &mut buf);
            }
            let cleaned = strip_footnote_backrefs(buf.trim());
            if !id.is_empty() {
                self.footnotes.push((id, cleaned));
            }
            return;
        }
        // Recurse into children with the same block context.
        for child in node.children() {
            self.render_block_or_inline(&child, out);
        }
    }

    fn render_heading(&mut self, node: &NodeRef, tag: &str, out: &mut String) {
        let mut level = tag
            .strip_prefix('h')
            .and_then(|n| n.parse::<usize>().ok())
            .unwrap_or(1)
            .clamp(1, 6);
        // Defuddle demotes all H1s to H2 in markdown output (keeping the
        // article title at the top of the document, not in the body).
        if level == 1 {
            level = 2;
        }
        let text = self.render_inline(node);
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        ensure_trailing_newlines(out, 2);
        for _ in 0..level {
            out.push('#');
        }
        out.push(' ');
        out.push_str(text);
        ensure_trailing_newlines(out, 2);
    }

    fn render_paragraph(&mut self, node: &NodeRef, out: &mut String) {
        let text = self.render_inline(node);
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        ensure_trailing_newlines(out, 2);
        out.push_str(text);
        ensure_trailing_newlines(out, 2);
    }

    fn render_blockquote(&mut self, node: &NodeRef, out: &mut String) {
        // Render children into a temporary buffer.
        let mut inner = String::new();
        for child in node.children() {
            self.render_block_or_inline(&child, &mut inner);
        }
        let inner = inner.trim();
        if inner.is_empty() {
            return;
        }
        ensure_trailing_newlines(out, 2);
        for line in inner.lines() {
            if line.is_empty() {
                out.push_str(">\n");
            } else {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
        }
        ensure_trailing_newlines(out, 2);
    }

    fn render_callout(&mut self, node: &NodeRef, out: &mut String) {
        let kind = attr(node, "data-callout")
            .or_else(|| {
                let class = attr(node, "class").unwrap_or_default();
                class
                    .split_whitespace()
                    .find_map(|c| c.strip_prefix("callout-").map(str::to_string))
            })
            .unwrap_or_default();
        let fold = attr(node, "data-callout-fold").unwrap_or_default();
        let fold_marker = match fold.as_str() {
            "+" => "+",
            "-" => "-",
            _ => "",
        };

        // Find title and content children.
        let mut title = String::new();
        let mut content_node: Option<NodeRef> = None;
        for child in node.descendants() {
            if !child.as_element().is_some() {
                continue;
            }
            if title.is_empty() && has_class(&child, "callout-title-inner") {
                title = self.render_inline(&child).trim().to_string();
            }
            if content_node.is_none() && has_class(&child, "callout-content") {
                content_node = Some(child.clone());
            }
        }
        if title.is_empty() {
            // Fallback: capitalize the type.
            let mut s = kind.clone();
            if let Some(first) = s.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            title = s;
        }

        ensure_trailing_newlines(out, 2);
        out.push_str("> [!");
        out.push_str(&kind);
        out.push(']');
        out.push_str(fold_marker);
        if !title.is_empty() {
            out.push(' ');
            out.push_str(&title);
        }
        out.push('\n');

        // Render content into a temp buffer and prefix every line with "> ".
        let mut body = String::new();
        if let Some(cn) = content_node {
            for child in cn.children() {
                self.render_block_or_inline(&child, &mut body);
            }
        }
        let body = body.trim();
        if !body.is_empty() {
            for line in body.lines() {
                if line.is_empty() {
                    out.push_str(">\n");
                } else {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
        ensure_trailing_newlines(out, 2);
    }

    fn render_list(&mut self, node: &NodeRef, out: &mut String) {
        let ordered = is_tag(node, "ol");
        let start: u32 = attr(node, "start")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        self.list_stack.push(ListFrame {
            ordered,
            next: start,
        });
        // Footnote-list special-case: ol with parent #footnotes (or aside.footnotes / class footnotes).
        if ordered && is_footnote_list(node) {
            self.list_stack.pop();
            self.collect_footnote_list(node);
            return;
        }

        let mut buf = String::new();
        for child in node.children() {
            if !is_tag(&child, "li") {
                continue;
            }
            self.render_list_item(&child, &mut buf);
        }
        self.list_stack.pop();

        if buf.trim().is_empty() {
            return;
        }
        // If we are nested inside another list, don't add the surrounding
        // blank lines — the parent <li> handles its own spacing.
        if self.list_stack.is_empty() {
            ensure_trailing_newlines(out, 2);
            out.push_str(buf.trim_end_matches('\n'));
            ensure_trailing_newlines(out, 2);
        } else {
            out.push_str(&buf);
        }
    }

    fn render_list_item(&mut self, node: &NodeRef, out: &mut String) {
        let depth = self.list_stack.len().saturating_sub(1);
        let indent = "\t".repeat(depth);
        let frame = self.list_stack.last().copied();

        // Compute marker.
        let marker = if let Some(f) = frame {
            if f.ordered {
                format!("{}. ", f.next)
            } else {
                "- ".to_string()
            }
        } else {
            "- ".to_string()
        };
        if let Some(top) = self.list_stack.last_mut() {
            if top.ordered {
                top.next += 1;
            }
        }

        // Task list checkbox.
        let checkbox = task_list_marker(node);

        // Render the body. We need to split out any nested lists so they go
        // on their own lines indented one level deeper.
        let mut inline_buf = String::new();
        let mut nested_buf = String::new();
        for child in node.children() {
            if let Some(_text) = child.as_text() {
                inline_buf.push_str(&self.render_inline_text(&child.as_text().unwrap().borrow()));
                continue;
            }
            if !child.as_element().is_some() {
                continue;
            }
            let tg = tag_name(&child);
            match tg.as_str() {
                "ul" | "ol" => {
                    // Flush nested list into nested_buf.
                    let mut nb = String::new();
                    self.render_list(&child, &mut nb);
                    nested_buf.push_str(nb.trim_end_matches('\n'));
                    nested_buf.push('\n');
                }
                "p" => {
                    // First paragraph stays inline; subsequent paragraphs become continuation lines.
                    let inner = self.render_inline(&child);
                    if inline_buf.trim().is_empty() {
                        inline_buf.clear();
                        inline_buf.push_str(inner.trim());
                    } else {
                        nested_buf.push('\n');
                        nested_buf.push_str(inner.trim());
                        nested_buf.push('\n');
                    }
                }
                "br" => inline_buf.push_str("  \n"),
                _ if util::is_inline_tag(&tg) => {
                    self.render_inline_node(&child, &mut inline_buf);
                }
                _ => {
                    // Block child (blockquote, pre, table, etc.) — render as
                    // continuation.
                    let mut nb = String::new();
                    self.render_block_or_inline(&child, &mut nb);
                    nested_buf.push_str(nb.trim_end_matches('\n'));
                    nested_buf.push('\n');
                }
            }
        }

        let body_first = inline_buf.trim();
        if body_first.is_empty() && nested_buf.trim().is_empty() {
            return;
        }

        out.push_str(&indent);
        out.push_str(&marker);
        if let Some(c) = checkbox {
            out.push_str(c);
            out.push(' ');
        }
        out.push_str(body_first);
        out.push('\n');

        let cont_indent = format!("{indent}\t");
        for line in nested_buf.lines() {
            if line.is_empty() {
                out.push('\n');
            } else if line.starts_with('\t') || line.starts_with("- ") || is_ordered_marker(line) {
                // Already a list line — preserve indentation.
                out.push_str(&cont_indent);
                out.push_str(line);
                out.push('\n');
            } else {
                out.push_str(&cont_indent);
                out.push_str(line);
                out.push('\n');
            }
        }
    }

    fn render_pre(&mut self, node: &NodeRef, out: &mut String) {
        // Drop pre wrappers used by code-highlighting figures so we still pick up
        // the inner code element.
        let inner_code = node.descendants().find(|n| is_tag(n, "code"));
        let lang = code::detect_language(node, inner_code.as_ref()).unwrap_or_default();
        let body = code::extract_code_text(node);
        if body.trim().is_empty() && lang.is_empty() {
            return;
        }
        ensure_trailing_newlines(out, 2);
        out.push_str("```");
        out.push_str(&lang);
        out.push('\n');
        out.push_str(&body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
        out.push_str("```");
        ensure_trailing_newlines(out, 2);
    }

    fn render_table(&mut self, node: &NodeRef, out: &mut String) {
        let kind = tables::classify(node);
        match kind {
            tables::TableKind::Empty => {}
            tables::TableKind::Layout => {
                // Treat each row's first cell as a transparent block.
                for row in node.descendants().filter(|n| is_tag(n, "tr")) {
                    for cell in row.children().filter(|c| is_any_tag(c, &["td", "th"])) {
                        for child in cell.children() {
                            self.render_block_or_inline(&child, out);
                        }
                    }
                }
            }
            tables::TableKind::Complex => {
                // Emit as raw HTML — markdown allows it, and Defuddle does the
                // same. We serialize the cleaned table.
                ensure_trailing_newlines(out, 2);
                let html = serialize_node(node);
                out.push_str(html.trim());
                ensure_trailing_newlines(out, 2);
            }
            tables::TableKind::Simple => {
                let prev = self.in_table;
                self.in_table = true;
                let table_md = tables::render_simple(node, |cell| {
                    let mut sub = Renderer::new();
                    sub.in_table = true;
                    sub.render_inline(cell)
                });
                self.in_table = prev;
                if !table_md.trim().is_empty() {
                    ensure_trailing_newlines(out, 2);
                    out.push_str(table_md.trim_end_matches('\n'));
                    ensure_trailing_newlines(out, 2);
                }
            }
        }
    }

    fn render_figure(&mut self, node: &NodeRef, out: &mut String) {
        if figures::figure_is_content_wrapper(node) {
            // Render children as ordinary blocks.
            for child in node.children() {
                self.render_block_or_inline(&child, out);
            }
            return;
        }

        // Find the first <img> and an optional <figcaption>.
        let img = node.descendants().find(|n| is_tag(n, "img"));
        let caption = node.descendants().find(|n| is_tag(n, "figcaption"));

        let img_md = img
            .as_ref()
            .map(|i| self.render_image(i))
            .unwrap_or_default();
        let cap_md = caption
            .as_ref()
            .map(|c| self.render_inline(c))
            .unwrap_or_default();

        if img_md.is_empty() && cap_md.trim().is_empty() {
            return;
        }
        ensure_trailing_newlines(out, 2);
        if !img_md.is_empty() {
            out.push_str(&img_md);
            ensure_trailing_newlines(out, 2);
        }
        if !cap_md.trim().is_empty() {
            out.push_str(cap_md.trim());
            ensure_trailing_newlines(out, 2);
        }
    }

    fn render_dl(&mut self, node: &NodeRef, out: &mut String) {
        // Render each <dt> as one paragraph and each <dd> as the next. This
        // matches Defuddle's behavior, which treats `<dl>` as a content
        // wrapper rather than synthesizing the colon-prefix definition list
        // syntax (which most Markdown flavors don't support).
        for child in node.children() {
            if !child.as_element().is_some() {
                continue;
            }
            let tg = tag_name(&child);
            if tg == "dt" || tg == "dd" {
                self.render_paragraph(&child, out);
            }
        }
    }

    /// Render a node's children as a single string of inline markdown (no block
    /// breaks).
    fn render_inline(&mut self, node: &NodeRef) -> String {
        let mut buf = String::new();
        for child in node.children() {
            self.render_inline_node(&child, &mut buf);
        }
        // Collapse any internal hard-break of the form `   \n` already produced.
        // But preserve `  \n` markers (two spaces + newline).
        buf
    }

    fn render_inline_node(&mut self, node: &NodeRef, out: &mut String) {
        if let Some(text) = node.as_text() {
            let raw = text.borrow();
            out.push_str(&self.render_inline_text(&raw));
            return;
        }
        let Some(_) = node.as_element() else {
            return;
        };

        let tag = tag_name(node);
        match tag.as_str() {
            "br" => out.push_str("  \n"),
            "strong" | "b" => {
                let inner = self.render_inline(node);
                if !inner.trim().is_empty() {
                    out.push_str("**");
                    out.push_str(inner.trim());
                    out.push_str("**");
                }
            }
            "em" | "i" => {
                let inner = self.render_inline(node);
                if !inner.trim().is_empty() {
                    out.push('*');
                    out.push_str(inner.trim());
                    out.push('*');
                }
            }
            "del" | "s" | "strike" => {
                let inner = self.render_inline(node);
                if !inner.trim().is_empty() {
                    out.push_str("~~");
                    out.push_str(inner.trim());
                    out.push_str("~~");
                }
            }
            "mark" => {
                let inner = self.render_inline(node);
                if !inner.trim().is_empty() {
                    out.push_str("==");
                    out.push_str(inner.trim());
                    out.push_str("==");
                }
            }
            "code" => self.render_inline_code(node, out),
            "a" => self.render_anchor(node, out),
            "img" => out.push_str(&self.render_image(node)),
            "sup" => self.render_sup(node, out),
            "sub" => {
                let inner = self.render_inline(node);
                if !inner.trim().is_empty() {
                    out.push_str("<sub>");
                    out.push_str(inner.trim());
                    out.push_str("</sub>");
                }
            }
            "math" => {
                if let Some(latex) = mathml_latex(node) {
                    if self.in_table {
                        out.push('$');
                        out.push_str(&latex);
                        out.push('$');
                    } else {
                        let display = attr(node, "display").as_deref() == Some("block");
                        if display {
                            out.push_str("\n\n$$\n");
                            out.push_str(&latex);
                            out.push_str("\n$$\n\n");
                        } else {
                            out.push('$');
                            out.push_str(&latex);
                            out.push('$');
                        }
                    }
                }
            }
            "span" | "u" | "small" | "abbr" | "cite" | "dfn" | "kbd" | "samp" | "var" | "time"
            | "data" | "label" | "ruby" | "rp" | "rt" | "tt" | "ins" | "q" | "bdi" | "bdo" => {
                // Special-case KaTeX wrappers: emit LaTeX from data-latex / annotation.
                if has_any_class(node, &["math", "katex", "katex-display"]) {
                    if let Some(latex) = katex_latex(node) {
                        let is_display =
                            has_class(node, "katex-display") || has_class(node, "math-display");
                        if is_display && !self.in_table {
                            out.push_str("\n\n$$\n");
                            out.push_str(&latex);
                            out.push_str("\n$$\n\n");
                        } else {
                            out.push('$');
                            out.push_str(&latex);
                            out.push('$');
                        }
                        return;
                    }
                }
                // Transparent passthrough.
                let inner = self.render_inline(node);
                out.push_str(&inner);
            }
            "iframe" => {
                // Embed transformations are handled upstream by standardize.rs;
                // anything that survived that pass we serialize raw.
                out.push_str(&serialize_node(node));
            }
            "button" => {
                let inner = self.render_inline(node);
                out.push_str(&inner);
            }
            "script" | "style" | "noscript" | "template" => {}
            // Block elements appearing in inline context — render their text content.
            "p" | "div" | "section" | "article" => {
                let inner = self.render_inline(node);
                if !inner.is_empty() {
                    out.push_str(&inner);
                }
            }
            _ => {
                let inner = self.render_inline(node);
                out.push_str(&inner);
            }
        }
    }

    fn render_inline_text(&self, raw: &str) -> String {
        if self.in_pre {
            return raw.to_string();
        }
        // Collapse runs of whitespace (including newlines) to a single space.
        // Drop zero-width non-printing scaffolding chars (BOM / ZWNBSP).
        let mut buf = String::with_capacity(raw.len());
        let mut prev_space = false;
        for c in raw.chars() {
            // Strip BOM, ZWJ/ZWNJ at the start of a token only? Defuddle
            // keeps ZWJ/ZWNJ but strips U+FEFF unconditionally.
            if c == '\u{FEFF}' {
                continue;
            }
            if c.is_whitespace() {
                if !prev_space {
                    buf.push(' ');
                }
                prev_space = true;
            } else {
                buf.push(c);
                prev_space = false;
            }
        }
        escape_md_text(&buf)
    }

    fn render_inline_code(&mut self, node: &NodeRef, out: &mut String) {
        // Inline code (not inside a <pre>) — render text content with backtick
        // escaping.
        let content = node.text_contents();
        let content = content.trim();
        if content.is_empty() {
            return;
        }
        // Choose enough backticks to wrap.
        let mut max_ticks = 0usize;
        let mut run = 0usize;
        for c in content.chars() {
            if c == '`' {
                run += 1;
                if run > max_ticks {
                    max_ticks = run;
                }
            } else {
                run = 0;
            }
        }
        let ticks = "`".repeat(max_ticks + 1);
        out.push_str(&ticks);
        // Add a space if content begins/ends with a backtick.
        let pad_start = content.starts_with('`');
        let pad_end = content.ends_with('`');
        if pad_start {
            out.push(' ');
        }
        out.push_str(content);
        if pad_end {
            out.push(' ');
        }
        out.push_str(&ticks);
    }

    fn render_anchor(&mut self, node: &NodeRef, out: &mut String) {
        // Footnote ref?
        if let Some(id) = links::footnote_ref_id(node) {
            // Check if this anchor wraps a <sup> — Defuddle treats those as
            // footnote refs.
            if node.descendants().any(|n| is_tag(&n, "sup"))
                || links::is_backref(node) == false
                    && node
                        .text_contents()
                        .trim()
                        .chars()
                        .all(|c| c.is_ascii_digit() || c == '↩')
            {
                if !id.is_empty() {
                    out.push_str("[^");
                    out.push_str(&id);
                    out.push(']');
                    return;
                }
            }
        }

        if links::is_backref(node) {
            return;
        }

        let inner = self.render_inline(node);
        let inner = inner.trim();
        let Some(href) = links::link_href(node) else {
            // No href — just emit the inner text.
            out.push_str(inner);
            return;
        };
        if inner.is_empty() {
            // Skip empty links (would emit `[](url)`).
            return;
        }

        let title = attr(node, "title");
        let resolved = self.resolve_url(&href);
        out.push('[');
        out.push_str(inner);
        out.push(']');
        out.push('(');
        out.push_str(&decode_entities(&resolved));
        if let Some(t) = title {
            if !t.is_empty() {
                out.push_str(" \"");
                out.push_str(&t.replace('"', "\\\""));
                out.push('"');
            }
        }
        out.push(')');
    }

    fn render_sup(&mut self, node: &NodeRef, out: &mut String) {
        // Footnote ref pattern: <sup id="fnref:N"> or class footnote-ref.
        if let Some(id) = footnote_id_from_sup(node) {
            self.emit_footnote_ref(out, &id);
            return;
        }
        // <sup><a href="#fn:N">...</a></sup>
        if let Some(anchor) = node.descendants().find(|n| is_tag(n, "a")) {
            if let Some(id) = links::footnote_ref_id(&anchor) {
                self.emit_footnote_ref(out, &id);
                return;
            }
        }
        // `<sup>N</sup>` whose plain-text content is a small integer is
        // treated as a footnote reference, regardless of class. This matches
        // what Defuddle does when the surrounding document declares
        // footnotes elsewhere.
        let txt = node.text_contents();
        let trimmed = txt.trim();
        if !trimmed.is_empty() && trimmed.len() <= 4 && trimmed.chars().all(|c| c.is_ascii_digit())
        {
            self.emit_footnote_ref(out, trimmed);
            return;
        }
        let inner = self.render_inline(node);
        if !inner.trim().is_empty() {
            out.push_str("<sup>");
            out.push_str(inner.trim());
            out.push_str("</sup>");
        }
    }

    /// Resolve a possibly-relative URL against the renderer's base URL.
    fn resolve_url(&self, href: &str) -> String {
        let trimmed = href.trim();
        // Absolute URLs and special schemes pass through.
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with("mailto:")
            || trimmed.starts_with("tel:")
            || trimmed.starts_with("data:")
            || trimmed.starts_with('#')
        {
            return trimmed.to_string();
        }
        let Some(base) = self.base_url.as_ref() else {
            return trimmed.to_string();
        };
        let Ok(parsed) = url::Url::parse(base) else {
            return trimmed.to_string();
        };
        match parsed.join(trimmed) {
            Ok(u) => u.to_string(),
            Err(_) => trimmed.to_string(),
        }
    }

    /// Emit `[^id]`. Add a leading space iff the immediately preceding
    /// character is a word/closing-inline character; otherwise preserve
    /// existing whitespace verbatim.
    fn emit_footnote_ref(&self, out: &mut String, id: &str) {
        if let Some(c) = out.chars().last() {
            if c.is_alphanumeric() || c == '`' || c == ')' || c == ']' || c == '*' {
                out.push(' ');
            }
        }
        out.push_str("[^");
        out.push_str(id);
        out.push(']');
    }

    fn render_image(&mut self, node: &NodeRef) -> String {
        let Some(src) = figures::best_img_src(node) else {
            return String::new();
        };
        // Skip lazy-load placeholders.
        if src.starts_with("data:") {
            return String::new();
        }
        let resolved = self.resolve_url(&src);
        let alt = attr(node, "alt").unwrap_or_default();
        let title = attr(node, "title").unwrap_or_default();
        let mut out = String::from("![");
        out.push_str(&alt);
        out.push(']');
        out.push('(');
        out.push_str(&decode_entities(&resolved));
        if !title.is_empty() {
            out.push_str(" \"");
            out.push_str(&title.replace('"', "\\\""));
            out.push('"');
        }
        out.push(')');
        out
    }

    fn collect_footnote_list(&mut self, ol: &NodeRef) {
        let start: u32 = attr(ol, "start")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        let mut idx = start;
        for li in ol.children().filter(|c| is_tag(c, "li")) {
            // Prefer an explicit id; otherwise number from the ol's start.
            let id = attr(&li, "id")
                .map(|raw| {
                    let stripped = raw
                        .strip_prefix("fn:")
                        .or_else(|| raw.strip_prefix("fn-"))
                        .or_else(|| raw.strip_prefix("footnote-"))
                        .or_else(|| raw.strip_prefix("cite_note-"))
                        .or_else(|| {
                            raw.strip_prefix("fn")
                                .filter(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit()))
                        })
                        .unwrap_or(&raw);
                    stripped.split('-').next().unwrap_or(stripped).to_string()
                })
                .unwrap_or_else(|| idx.to_string());
            idx += 1;
            // Render the li's children as block content, then strip backref tails.
            let mut buf = String::new();
            for child in li.children() {
                self.render_block_or_inline(&child, &mut buf);
            }
            let cleaned = strip_footnote_backrefs(buf.trim());
            self.footnotes.push((id, cleaned));
        }
    }
}

fn task_list_marker(li: &NodeRef) -> Option<&'static str> {
    let mut input = None;
    for d in li.descendants() {
        if is_tag(&d, "input")
            && attr(&d, "type")
                .map(|t| t.eq_ignore_ascii_case("checkbox"))
                .unwrap_or(false)
        {
            input = Some(d);
            break;
        }
    }
    let input = input?;
    let checked = attr(&input, "checked").is_some()
        || attr(&input, "data-checked")
            .map(|v| v != "false")
            .unwrap_or(false);
    Some(if checked { "[x]" } else { "[ ]" })
}

fn is_ordered_marker(line: &str) -> bool {
    let trimmed = line.trim_start_matches('\t');
    let mut chars = trimmed.chars();
    let mut saw_digit = false;
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() {
            saw_digit = true;
        } else if c == '.' && saw_digit {
            return matches!(chars.next(), Some(' '));
        } else {
            break;
        }
    }
    false
}

fn is_callout(node: &NodeRef) -> bool {
    if !is_tag(node, "div") && !is_tag(node, "blockquote") {
        return false;
    }
    if attr(node, "data-callout").is_some() {
        return true;
    }
    if let Some(class) = attr(node, "class") {
        if class.split_whitespace().any(|t| t == "callout") {
            return true;
        }
    }
    false
}

fn is_footnote_list(ol: &NodeRef) -> bool {
    if has_class(ol, "footnotes-list") {
        return true;
    }
    let mut cur = ol.parent();
    while let Some(p) = cur {
        if let Some(id) = attr(&p, "id") {
            if id.eq_ignore_ascii_case("footnotes") {
                return true;
            }
        }
        if has_class(&p, "footnotes") || has_class(&p, "footnote") {
            return true;
        }
        if is_tag(&p, "aside") {
            // An <aside> wrapping an <ol> is the canonical "footnote aside"
            // pattern Defuddle recognizes.
            return true;
        }
        cur = p.parent();
    }
    false
}

fn footnote_id_from_sup(sup: &NodeRef) -> Option<String> {
    let id = attr(sup, "id")?;
    let stripped = id
        .strip_prefix("fnref:")
        .or_else(|| id.strip_prefix("fnref"))
        .or_else(|| id.strip_prefix("footnote-ref-"))
        .or_else(|| id.strip_prefix("cite_ref-"))?;
    Some(stripped.split('-').next().unwrap_or(stripped).to_string())
}

fn strip_footnote_backrefs(s: &str) -> String {
    static BACKREF_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\s*↩(?:︎)?\s*$").expect("backref regex"));
    BACKREF_RE.replace(s, "").into_owned()
}

/// Extract LaTeX from a `<math>` element using the priority Defuddle uses.
fn mathml_latex(node: &NodeRef) -> Option<String> {
    if let Some(v) = attr(node, "data-latex") {
        return Some(v);
    }
    if let Some(v) = attr(node, "alttext") {
        return Some(v);
    }
    // Look for an annotation child.
    for d in node.descendants() {
        if is_tag(&d, "annotation") && attr(&d, "encoding").as_deref() == Some("application/x-tex")
        {
            let txt = d.text_contents().trim().to_string();
            if !txt.is_empty() {
                return Some(txt);
            }
        }
    }
    let txt = node.text_contents().trim().to_string();
    if txt.is_empty() { None } else { Some(txt) }
}

fn katex_latex(node: &NodeRef) -> Option<String> {
    if let Some(v) = attr(node, "data-latex") {
        return Some(v);
    }
    for d in node.descendants() {
        if is_tag(&d, "annotation") && attr(&d, "encoding").as_deref() == Some("application/x-tex")
        {
            let txt = d.text_contents().trim().to_string();
            if !txt.is_empty() {
                return Some(txt);
            }
        }
    }
    None
}

/// Serialize a single node to HTML via kuchikiki.
fn serialize_node(node: &NodeRef) -> String {
    let mut buf: Vec<u8> = Vec::new();
    if node.serialize(&mut buf).is_ok() {
        String::from_utf8_lossy(&buf).into_owned()
    } else {
        String::new()
    }
}

fn strip_leading_title_heading(md: &str, title: &str) -> String {
    let trimmed = md.trim_start();
    let lead = md.len() - trimmed.len();
    let mut lines = trimmed.lines();
    let Some(first) = lines.next() else {
        return md.to_string();
    };
    // Match `# X` or `## X`.
    let body = first
        .strip_prefix("# ")
        .or_else(|| first.strip_prefix("## "));
    let Some(body) = body else {
        return md.to_string();
    };
    if !heading_matches_title(body.trim(), title.trim()) {
        return md.to_string();
    }
    let mut new_start = lead + first.len();
    while md[new_start..].starts_with('\n') {
        new_start += 1;
    }
    md[new_start..].to_string()
}

/// Reverse `escape_md_text`'s backslash escaping for use during string
/// equality checks against raw (unescaped) titles.
fn unescape_md(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if matches!(next, '\\' | '`' | '[' | ']' | '_' | '*') {
                    out.push(chars.next().expect("peeked"));
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
}

fn heading_matches_title(h: &str, title: &str) -> bool {
    // The heading may have been markdown-escaped (e.g. `ftnt\_ref`) while the
    // title is the raw HTML title (`ftnt_ref`). Normalize by stripping
    // backslash-escapes used by `escape_md_text`.
    let h_norm = unescape_md(h);
    let h = h_norm.as_str();
    if h == title {
        return true;
    }
    if h.eq_ignore_ascii_case(title) {
        return true;
    }
    // Tolerate the title being a "Heading - Site Name" or "Heading | Site"
    // suffixed form.
    let t_low = title.to_lowercase();
    let h_low = h.to_lowercase();
    if t_low.starts_with(&h_low) {
        let rest = t_low[h_low.len()..].trim_start();
        if rest.starts_with('-')
            || rest.starts_with('|')
            || rest.starts_with('·')
            || rest.starts_with(':')
        {
            return true;
        }
    }
    // Conversely, the heading may be a fuller version of the (brand) title.
    if h_low.starts_with(&t_low) && t_low.len() < h_low.len() {
        return false; // brand-style title — not a match for the article H1.
    }
    false
}

/// Final cleanup pass.
fn post_process(md: &str, footnotes: &[(String, String)], title: &str) -> String {
    let mut s = md.to_string();

    // Strip a leading H2 (or H1, if any survived demotion) that matches the
    // article title. Defuddle's `stripLeadingH1`.
    if !title.is_empty() {
        s = strip_leading_title_heading(&s, title);
    }

    // Remove empty links: `[](url)` (preserve images `![](url)`).
    static EMPTY_LINK_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?m)(?:^|[^!])\[\]\([^)]*\)").expect("empty link regex"));
    s = EMPTY_LINK_RE
        .replace_all(&s, |caps: &regex::Captures| {
            // Preserve a leading non-`!` char (the regex captures it for backtracking).
            let m = &caps[0];
            // If first char isn't `[` it's the leading char to preserve.
            if let Some(first) = m.chars().next() {
                if first != '[' {
                    return first.to_string();
                }
            }
            String::new()
        })
        .into_owned();

    // Insert space between consecutive `!` markers where they would otherwise
    // be parsed as image syntax. Rust's `regex` crate doesn't support
    // lookahead, so we match-and-restore.
    static BANG_BANG_RE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"!(!\[|\[!\[)").expect("bang regex"));
    s = BANG_BANG_RE.replace_all(&s, "! $1").into_owned();

    // Collapse 3+ newlines to 2.
    static MULTI_NL_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\n{3,}").expect("nl regex"));
    s = MULTI_NL_RE.replace_all(&s, "\n\n").into_owned();

    // Append accumulated footnote definitions.
    if !footnotes.is_empty() {
        if !s.ends_with('\n') {
            s.push('\n');
        }
        if !s.ends_with("\n\n") {
            s.push('\n');
        }
        let mut first = true;
        for (id, body) in footnotes {
            if !first {
                s.push('\n');
            }
            first = false;
            s.push_str("[^");
            s.push_str(id);
            s.push_str("]: ");
            // Body should be single-line; replace newlines with spaces unless
            // it's intentionally multi-line markdown — for simplicity, join.
            let one_line = body
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            s.push_str(&one_line);
            s.push('\n');
        }
    }

    // Trim trailing whitespace per line (avoid disturbing two-space hard breaks
    // by only trimming 3+ trailing spaces).
    let cleaned: Vec<String> = s
        .lines()
        .map(|line| {
            // Preserve `  \n` hard breaks (exactly two trailing spaces).
            let trailing = line.chars().rev().take_while(|c| *c == ' ').count();
            if trailing == 2 {
                line.to_string()
            } else {
                line.trim_end().to_string()
            }
        })
        .collect();
    s = cleaned.join("\n");

    // Final trim of leading/trailing blank lines.
    s = s.trim().to_string();

    let _ = trim_end_newlines; // silence unused-import warning for re-export hygiene
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(input: &str) -> String {
        html_to_markdown(input)
    }

    #[test]
    fn paragraph() {
        assert_eq!(md("<p>hello world</p>"), "hello world");
    }

    #[test]
    fn heading_emphasis_link() {
        assert_eq!(
            md(
                "<h2>Title</h2><p>Some <em>emphasized</em> and <strong>bold</strong> text with <a href='https://x.test'>a link</a>.</p>"
            ),
            "## Title\n\nSome *emphasized* and **bold** text with [a link](https://x.test/)."
        );
    }

    #[test]
    fn unordered_list() {
        let out = md("<ul><li>one</li><li>two</li></ul>");
        assert_eq!(out, "- one\n- two");
    }

    #[test]
    fn ordered_list_start() {
        let out = md("<ol start=\"3\"><li>three</li><li>four</li></ol>");
        assert_eq!(out, "3. three\n4. four");
    }

    #[test]
    fn fenced_code_with_lang() {
        let out = md("<pre><code class=\"language-rust\">fn x() {}</code></pre>");
        assert_eq!(out, "```rust\nfn x() {}\n```");
    }

    #[test]
    fn empty_link_dropped() {
        let out = md("<p>see <a href=\"https://x.test\"></a> here</p>");
        assert!(!out.contains("[]("));
    }

    #[test]
    fn image_with_alt() {
        let out = md("<p><img src=\"a.png\" alt=\"alt\"></p>");
        assert!(out.contains("![alt](a.png)"));
    }

    #[test]
    fn blockquote_simple() {
        let out = md("<blockquote><p>hi</p><p>bye</p></blockquote>");
        assert!(out.starts_with("> hi\n>"));
    }

    #[test]
    fn callout_pre_with_tokens() {
        let html = r#"<pre class="language-markdown"><code class="language-markdown"><span class="token blockquote">&gt;</span> [!info] A callout title
<span class="token blockquote">&gt;</span> Here is body</code></pre>"#;
        let out = md(html);
        assert!(out.contains("> [!info] A callout title"), "got: {out}");
    }

    #[test]
    fn aside_footnote() {
        let html = r#"<p>Shrinking is useful.</p>
<aside><ol start="3"><li>See <a href="https://example.com/shrinking">this post</a> for details.</li></ol></aside>"#;
        let out = md(html);
        assert!(out.contains("[^3]: "), "got: {out}");
        assert!(out.contains("[this post]"), "lost link: {out}");
    }

    #[test]
    fn flex_row_code_block() {
        let html = r#"<pre class="flex flex-col"><div class="gap-2xs flex flex-row"><span class="text-end">1</span><div class="flex-1">AGENTS.md</div></div><div class="gap-2xs flex flex-row"><span class="text-end">2</span><div class="flex-1">ARCHITECTURE.md</div></div></pre>"#;
        let out = md(html);
        assert!(out.contains("AGENTS.md\nARCHITECTURE.md"), "got: {out}");
    }

    #[test]
    fn list_with_inline_code() {
        let html = "<ul><li>Written in <code>Lua</code></li><li>Asynchronous execution</li></ul>";
        let out = md(html);
        assert!(out.contains("- Written in `Lua`"), "got: {out}");
    }

    #[test]
    fn callout_basic() {
        let html = r#"<div data-callout="info" class="callout"><div class="callout-title"><div class="callout-title-inner">Hi</div></div><div class="callout-content"><p>body</p></div></div>"#;
        let out = md(html);
        assert!(out.contains("> [!info] Hi"), "got: {out}");
    }

    #[test]
    fn srcset_picks_highest() {
        let out =
            md("<img src=\"small.png\" srcset=\"a.png 100w, b.png 800w, c.png 400w\" alt=\"x\">");
        assert!(out.contains("b.png"), "got: {out}");
    }
}
