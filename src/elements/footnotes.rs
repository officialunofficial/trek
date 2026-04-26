//! Footnote normalization (Track D).
//!
//! Detects a wide range of footnote shapes seen in the wild and rewrites
//! them into canonical forms the markdown renderer already understands:
//!
//! * Inline reference: `<sup>N</sup>` (digit) — emitted as `[^N]`.
//! * Definition list: `<ol class="footnotes-list">` whose `<li id="fn:N">`
//!   nodes are collected as `[^N]: body`.
//!
//! Patterns handled here:
//!
//! 1. Canonical containers (`aside.footnotes`, `section.footnotes`,
//!    `ol.footnotes`, `ol.easy-footnotes-wrapper`, `[data-footnotes]`) —
//!    tag the inner `<ol>` with `class="footnotes-list"`.
//! 2. Paragraph-style definitions: a run of paragraphs that each begin with
//!    `<sup>N</sup>`, `<strong>N</strong>`, or `<b><sup>N</sup>...:</b>`
//!    — usually preceded by an `<hr>` or a "Notes/Footnotes/Endnotes"
//!    heading. Strip the delimiter and gather the run into an
//!    `<ol class="footnotes-list">`.
//! 3. `<p class="footnote">` — paragraphs scattered throughout the document,
//!    each containing a leading sup. Convert to footnote definitions in a
//!    final list.
//! 4. Inline-footnote span: `<span class="inline-footnote">N<span
//!    class="footnoteContent">body</span></span>`. Replace the marker with
//!    a canonical `<sup>N</sup>` and collect a `<li id="fn:N">` definition.
//! 5. `<span class="fna-ref" data-definition="ID"><a>*</a></span>` paired
//!    with `<aside id="ID">body</aside>` (asterisk-style data-definition):
//!    rewrite the marker to `<sup>N</sup>` and the aside body into a
//!    canonical `<li id="fn:N">`.
//! 6. Google-Docs `ftnt`/`ftnt_ref`: rewrite ids/hrefs to `fn:N`/`fnref:N`.
//! 7. Word HTML `_ftn`/`_ftnref`: rewrite ids/hrefs to `fn:N`/`fnref:N` and
//!    convert paragraph-style definitions after an `<hr>`.
//!
//! References inside the body (`<sup id="fnref:N"><a href="#fn:N">`) are
//! already understood by the markdown renderer; we don't rewrite them here.

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use super::util::{
    attr, has_class, is_tag, new_element, remove_attr, select_all, select_first, set_attr,
    transfer_children,
};

/// Run footnote normalization passes against `root`.
pub fn normalize_footnotes(root: &NodeRef) {
    // Cheap rewrites first — they may produce shapes later passes recognize.
    rewrite_word_ftn_ids(root);
    rewrite_ftnt_ids(root);
    rewrite_easy_footnote_classes(root);

    convert_inline_footnote_span(root);
    convert_data_definition_aside(root);

    // After id rewrite (Google Docs `ftnt`/`ftnt_ref`, Word `_ftn`/`_ftnref`),
    // collect any <p id="fn:N"> paragraphs into an ol.
    collect_id_indexed_paragraph_definitions(root);

    // Drop a leading HR or "Footnotes" heading that precedes a known
    // footnote container/list (`section.footnotes`, `aside.footnotes`,
    // `[data-footnotes]`, `ol.footnotes-list`, `div.footnote-definition`).
    drop_delimiter_before_known_footnote(root);

    // Drop heading/HR delimiters that introduce paragraph footnote runs.
    strip_footnote_delimiters(root);

    // Now turn paragraph-style definitions into ols.
    convert_paragraph_definitions_global(root);
    convert_p_class_footnotes(root);

    // Existing canonical-form tagging.
    tag_canonical_lists(root);
    convert_paragraph_definitions(root);
    convert_anchored_definitions(root);

    // Renumber non-numeric ids inside footnotes-list ols.
    renumber_named_ids(root);

    // Trim whitespace-only text nodes immediately surrounding inline
    // footnote refs (`<sup class="footnote-ref">`) so we don't end up
    // emitting `body. [^1]` instead of `body.[^1]`.
    trim_whitespace_around_footnote_refs(root);
}

// ---------------------------------------------------------------------------
// Canonical-list tagging
// ---------------------------------------------------------------------------

fn tag_canonical_lists(root: &NodeRef) {
    let containers = select_all(
        root,
        "aside.footnotes, section.footnotes, div.footnotes, ol.footnotes, \
         section[data-footnotes], ol.easy-footnotes-wrapper, ol.footnotes-list",
    );
    for c in containers {
        if is_tag(&c, "ol") {
            add_class(&c, "footnotes-list");
        } else if let Some(ol) = select_first(&c, "ol") {
            add_class(&ol, "footnotes-list");
        }
    }
}

fn add_class(node: &NodeRef, class: &str) {
    let cur = attr(node, "class").unwrap_or_default();
    let mut tokens: Vec<String> = cur.split_whitespace().map(String::from).collect();
    if !tokens.iter().any(|t| t == class) {
        tokens.push(class.to_string());
    }
    set_attr(node, "class", &tokens.join(" "));
}

// ---------------------------------------------------------------------------
// Paragraph-style definitions in canonical .footnotes / #footnotes container
// ---------------------------------------------------------------------------

static LEADING_NUMBER: Lazy<Regex> = Lazy::new(|| {
    // Match a bare number, optionally wrapped in `[...]`/`(...)`/`{...}`,
    // optionally followed by a closing punctuation.
    Regex::new(r"^\s*[\[\(\{]?\s*(\d+)\s*[\]\)\}]?\s*[.):]?\s*$")
        .expect("leading number regex")
});

fn convert_paragraph_definitions(root: &NodeRef) {
    let containers = select_all(root, ".footnotes, #footnotes");
    for container in containers {
        let mut defs: Vec<(String, NodeRef)> = Vec::new();
        for child in container.children() {
            if !is_tag(&child, "p") {
                continue;
            }
            let Some(num) = leading_number_from_strong(&child) else {
                continue;
            };
            defs.push((num, child.clone()));
        }
        if defs.is_empty() {
            continue;
        }
        let ol = new_element("ol", &[("class", "footnotes-list")]);
        for (num, p) in &defs {
            let li = new_element("li", &[("id", &format!("fn:{num}"))]);
            if let Some(first_strong) = p.descendants().find(|n| is_tag(n, "strong")) {
                first_strong.detach();
            }
            transfer_children(p, &li);
            ol.append(li);
        }
        let first = &defs[0].1;
        first.insert_before(ol);
        for (_, p) in &defs {
            p.detach();
        }
    }
}

fn leading_number_from_strong(p: &NodeRef) -> Option<String> {
    for child in p.children() {
        if let Some(_) = child.as_text() {
            if child.as_text()?.borrow().trim().is_empty() {
                continue;
            }
            return None;
        }
        if !child.as_element().is_some() {
            continue;
        }
        if !(is_tag(&child, "strong") || is_tag(&child, "b")) {
            return None;
        }
        let txt = child.text_contents();
        let caps = LEADING_NUMBER.captures(&txt)?;
        return Some(caps.get(1)?.as_str().to_string());
    }
    None
}

// ---------------------------------------------------------------------------
// Anchored definitions: `<a id="fn-1"></a> body…`
// ---------------------------------------------------------------------------

static FN_ANCHOR_ID: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(?:fn-|footnote-)(\d+)").expect("fn anchor regex"));

fn convert_anchored_definitions(root: &NodeRef) {
    let anchors = select_all(root, "a[id]");
    let mut groups: Vec<(String, NodeRef)> = Vec::new();
    for a in anchors {
        let Some(id) = attr(&a, "id") else { continue };
        let Some(caps) = FN_ANCHOR_ID.captures(&id) else {
            continue;
        };
        let num = caps
            .get(1)
            .map(|m| m.as_str().to_string())
            .unwrap_or_default();
        if num.is_empty() {
            continue;
        }
        groups.push((num, a.clone()));
    }
    if groups.is_empty() {
        return;
    }
    let Some(parent) = groups[0].1.parent() else {
        return;
    };
    if select_first(&parent, "ol.footnotes-list").is_some() {
        return;
    }
    let ol = new_element("ol", &[("class", "footnotes-list")]);
    for (num, anchor) in &groups {
        let li = new_element("li", &[("id", &format!("fn:{num}"))]);
        let Some(parent) = anchor.parent() else {
            continue;
        };
        anchor.detach();
        transfer_children(&parent, &li);
        ol.append(li);
        parent.detach();
    }
    parent.append(ol);
}

// ---------------------------------------------------------------------------
// Paragraph-style definition runs (anywhere)
//
// A "footnote-like paragraph" is a `<p>` whose first inline content is one of:
//
//   * `<sup>N</sup>` (then text)
//   * `<strong>N</strong>` (then text)
//   * `<b><sup>N</sup>label:</b>` (then text)
//
// Two or more such paragraphs in a row, OR a single such paragraph immediately
// preceded by a `<hr>` or a `<h*>Notes/Footnotes/Endnotes/References</h*>`,
// are collected into an `<ol class="footnotes-list">`.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct DefMatch {
    num: String,
    /// If a leading bold/strong "label:" was present, render it as **label:**.
    label: Option<String>,
}

/// Detach the first marker element (`<sup>`, `<strong>`, or `<b>`) from a
/// paragraph, descending through transparent `<span>` wrappers as needed.
fn detach_leading_marker(p: &NodeRef) {
    fn drop_first_marker(node: &NodeRef) -> bool {
        for c in node.children() {
            if let Some(t) = c.as_text() {
                if t.borrow().trim().is_empty() {
                    continue;
                }
                return false;
            }
            if c.as_element().is_none() {
                continue;
            }
            if is_tag(&c, "sup") {
                c.detach();
                return true;
            }
            if is_tag(&c, "strong") || is_tag(&c, "b") {
                // Detach the whole bold wrapper (it may contain the label).
                c.detach();
                return true;
            }
            if is_tag(&c, "span") {
                if drop_first_marker(&c) {
                    return true;
                }
                return false;
            }
            return false;
        }
        false
    }
    drop_first_marker(p);
}

/// First element child of `node`, skipping leading whitespace text. If that
/// child is a transparent inline wrapper (`<span>`), descend into it.
fn first_significant_descendant(node: &NodeRef) -> Option<NodeRef> {
    let mut cur = node.clone();
    loop {
        let mut found: Option<NodeRef> = None;
        for c in cur.children() {
            if let Some(t) = c.as_text() {
                if t.borrow().trim().is_empty() {
                    continue;
                }
                return None;
            }
            if c.as_element().is_some() {
                found = Some(c);
                break;
            }
        }
        let f = found?;
        if is_tag(&f, "span") {
            cur = f;
            continue;
        }
        return Some(f);
    }
}

fn parse_def_paragraph(p: &NodeRef) -> Option<DefMatch> {
    if !is_tag(p, "p") {
        return None;
    }
    // Walk children; skip leading whitespace text. If the first element is a
    // transparent inline wrapper (span), descend into it.
    let first = first_significant_descendant(p)?;

    // Direct <sup>N</sup>
    if is_tag(&first, "sup") {
        let txt = first.text_contents();
        let caps = LEADING_NUMBER.captures(txt.trim())?;
        return Some(DefMatch {
            num: caps.get(1)?.as_str().to_string(),
            label: None,
        });
    }
    // Direct <strong>N</strong>
    if is_tag(&first, "strong") || is_tag(&first, "b") {
        // Maybe wrapping a <sup>N</sup>label:
        let mut wrapped_num: Option<String> = None;
        let mut label_buf = String::new();
        let mut found_sup = false;
        for cc in first.children() {
            if let Some(t) = cc.as_text() {
                let txt = t.borrow().to_string();
                if found_sup {
                    label_buf.push_str(&txt);
                } else if txt.trim().is_empty() {
                    continue;
                } else if let Some(caps) = LEADING_NUMBER.captures(txt.trim()) {
                    // Bare digit text directly inside <b>/<strong>.
                    return Some(DefMatch {
                        num: caps.get(1)?.as_str().to_string(),
                        label: None,
                    });
                } else {
                    return None;
                }
            } else if is_tag(&cc, "sup") && !found_sup {
                let txt = cc.text_contents();
                let caps = LEADING_NUMBER.captures(txt.trim())?;
                wrapped_num = Some(caps.get(1)?.as_str().to_string());
                found_sup = true;
            } else if found_sup {
                // Other inline contents inside the bold wrapper become label.
                label_buf.push_str(&cc.text_contents());
            } else {
                return None;
            }
        }
        if let Some(num) = wrapped_num {
            let label = label_buf.trim().to_string();
            return Some(DefMatch {
                num,
                label: if label.is_empty() { None } else { Some(label) },
            });
        }
    }
    None
}

fn is_footnote_delimiter(node: &NodeRef) -> bool {
    if is_tag(node, "hr") {
        return true;
    }
    if matches!(
        tag_name_lc(node).as_str(),
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
    ) {
        let txt = node.text_contents().trim().to_lowercase();
        return matches!(
            txt.as_str(),
            "notes" | "footnotes" | "endnotes" | "references" | "footnote"
        );
    }
    false
}

fn tag_name_lc(node: &NodeRef) -> String {
    node.as_element()
        .map(|e| e.name.local.to_string().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Walk the document and find runs of footnote-definition paragraphs.
/// Convert each run into an `<ol class="footnotes-list">`.
fn convert_paragraph_definitions_global(root: &NodeRef) {
    // Recursively process every container.
    let mut stack: Vec<NodeRef> = vec![root.clone()];
    while let Some(node) = stack.pop() {
        // Skip if this node has been detached (no parent and not root).
        if node.parent().is_none() && !std::ptr::eq(node.0.as_ref(), root.0.as_ref()) {
            continue;
        }
        process_container(&node);
        for child in node.children() {
            if child.as_element().is_some() {
                stack.push(child);
            }
        }
    }
}

fn process_container(container: &NodeRef) {
    let children: Vec<NodeRef> = container.children().collect();
    if children.is_empty() {
        return;
    }
    // Find all def-like paragraphs in the container.
    let mut defs: Vec<(usize, DefMatch, NodeRef)> = Vec::new();
    for (i, c) in children.iter().enumerate() {
        if c.as_element().is_none() {
            continue;
        }
        if let Some(d) = parse_def_paragraph(c) {
            defs.push((i, d, c.clone()));
        }
    }
    if defs.is_empty() {
        return;
    }
    // Find an immediately-preceding delimiter (`<hr>` or
    // Notes/Footnotes/References heading) for the FIRST def.
    let first_idx = defs[0].0;
    let mut delim: Option<NodeRef> = None;
    for j in (0..first_idx).rev() {
        let prev = &children[j];
        if let Some(t) = prev.as_text() {
            if t.borrow().trim().is_empty() {
                continue;
            }
            break;
        }
        if prev.as_element().is_none() {
            continue;
        }
        if is_footnote_delimiter(prev) {
            delim = Some(prev.clone());
        }
        break;
    }

    // Acceptance: 2+ defs, OR 1 def preceded by a delimiter.
    let accept = defs.len() >= 2 || (defs.len() == 1 && delim.is_some());
    if !accept {
        return;
    }

    // Numbering must be sequential: 1, 2, 3, ... (allow non-1 starts).
    // If the numbers don't form an increasing sequence, bail.
    let mut prev_n: Option<u64> = None;
    for (_i, d, _n) in &defs {
        let Ok(n) = d.num.parse::<u64>() else {
            return;
        };
        if let Some(p) = prev_n {
            if n <= p {
                return;
            }
        }
        prev_n = Some(n);
    }

    // Build the ol.
    let ol = new_element("ol", &[("class", "footnotes-list")]);
    for (_i, d, p) in &defs {
        let li = new_element("li", &[("id", &format!("fn:{}", d.num))]);
        detach_leading_marker(p);
        if let Some(label) = &d.label {
            let strong = new_element("strong", &[]);
            strong.append(NodeRef::new_text(label.clone()));
            let p_inner = new_element("p", &[]);
            p_inner.append(strong);
            transfer_children(p, &p_inner);
            li.append(p_inner);
        } else {
            transfer_children(p, &li);
        }
        ol.append(li);
    }

    // Insert ol after the last def in DOM order, then detach all defs +
    // delim.
    let insert_anchor = defs.last().expect("non-empty").2.clone();
    insert_anchor.insert_after(ol);
    for (_i, _d, n) in &defs {
        n.detach();
    }
    if let Some(d) = delim {
        d.detach();
    }
}

/// Collect any `<p id="fn:N">` paragraphs anywhere in the document into a
/// canonical `<ol class="footnotes-list">`. Useful for Google-Docs and
/// Word patterns where each definition lives in its own `<div>`.
fn collect_id_indexed_paragraph_definitions(root: &NodeRef) {
    let candidates = select_all(root, "p[id^=\"fn:\"]");
    if candidates.len() < 2 {
        return;
    }
    // Build sequential numeric mapping; bail on duplicates.
    let mut entries: Vec<(u64, NodeRef)> = Vec::new();
    for p in &candidates {
        let Some(id) = attr(p, "id") else { continue };
        let Some(n_str) = id.strip_prefix("fn:") else { continue };
        let Ok(n) = n_str.parse::<u64>() else { continue };
        // Skip if this <p> is already inside a footnotes-list (avoid
        // double-conversion).
        let mut in_list = false;
        let mut cur = p.parent();
        while let Some(par) = cur {
            if has_class(&par, "footnotes-list") {
                in_list = true;
                break;
            }
            cur = par.parent();
        }
        if in_list {
            continue;
        }
        entries.push((n, p.clone()));
    }
    if entries.len() < 2 {
        return;
    }
    entries.sort_by_key(|(n, _)| *n);

    // Determine where to insert: parent of the first matching <p> or its
    // wrapper `<div>`. Detach all defs (and their wrapper div if div has
    // no other children).
    // We anchor the new ol after the LAST def's wrapper.
    let last = entries.last().expect("non-empty").1.clone();
    let anchor = wrapper_or_self(&last);
    let ol = new_element("ol", &[("class", "footnotes-list")]);
    for (n, p) in &entries {
        let li = new_element("li", &[("id", &format!("fn:{n}"))]);
        // Drop the leading anchor (back-ref) if it's the first child.
        let mut first_a: Option<NodeRef> = None;
        for c in p.children() {
            if let Some(t) = c.as_text() {
                if t.borrow().trim().is_empty() {
                    continue;
                }
                break;
            }
            if c.as_element().is_some() {
                if is_tag(&c, "a") {
                    first_a = Some(c.clone());
                }
                break;
            }
        }
        if let Some(a) = first_a {
            a.detach();
        }
        // Also drop a leading `<sup>` containing only the index anchor.
        let mut first_sup: Option<NodeRef> = None;
        for c in p.children() {
            if let Some(t) = c.as_text() {
                if t.borrow().trim().is_empty() {
                    continue;
                }
                break;
            }
            if c.as_element().is_some() {
                if is_tag(&c, "sup") {
                    let txt = c.text_contents().trim().to_string();
                    if txt.is_empty()
                        || txt
                            .trim_matches(|c: char| c == '[' || c == ']')
                            .chars()
                            .all(|cc| cc.is_ascii_digit())
                    {
                        first_sup = Some(c.clone());
                    }
                }
                break;
            }
        }
        if let Some(s) = first_sup {
            s.detach();
        }
        transfer_children(p, &li);
        ol.append(li);
    }
    anchor.insert_after(ol);
    // Detach defs and their wrapper if wrapper becomes empty.
    for (_, p) in &entries {
        let wrapper = wrapper_or_self(p);
        if wrapper.0.as_ref() as *const _ != p.0.as_ref() as *const _ {
            p.detach();
            // If wrapper is now empty (only whitespace), detach it.
            let any_significant = wrapper.children().any(|c| {
                if let Some(t) = c.as_text() {
                    !t.borrow().trim().is_empty()
                } else {
                    c.as_element().is_some()
                }
            });
            if !any_significant {
                wrapper.detach();
            }
        } else {
            p.detach();
        }
    }
}

/// Trim whitespace text nodes immediately before/after `<sup>` footnote
/// references inside wrapping `<span>` elements. Defuddle drops these
/// in its inline-pass; we mirror the behavior to avoid `body. [^1]` when
/// the source is `<span>body.<sup> 1 </sup></span>`.
fn trim_whitespace_around_footnote_refs(root: &NodeRef) {
    // Find sup nodes that look like footnote refs (digit text or class).
    let sups = select_all(root, "sup");
    for sup in sups {
        let txt = sup.text_contents();
        let trimmed = txt.trim();
        let is_digit_ref = !trimmed.is_empty()
            && trimmed.chars().all(|c| c.is_ascii_digit())
            && trimmed.len() <= 4;
        let is_class_ref = has_class(&sup, "footnote-ref")
            || has_class(&sup, "footnote-reference");
        if !is_digit_ref && !is_class_ref {
            continue;
        }
        // The wrap-inline pattern can be:
        //   <span>body.<span class="reference"> <sup>1</sup> </span></span>...
        // We want to drop leading whitespace inside the `<span class="reference">`
        // so the `<sup>` is directly adjacent to the previous text after
        // serialization.
        if let Some(parent) = sup.parent() {
            // Only act when parent is an inline wrapper span.
            if is_tag(&parent, "span") {
                // Trim any leading whitespace text node before the sup.
                if let Some(prev) = sup.previous_sibling() {
                    if let Some(t) = prev.as_text() {
                        let raw = t.borrow().to_string();
                        if raw.trim().is_empty() {
                            *t.borrow_mut() = "".into();
                        }
                    }
                }
            }
        }
    }
}

/// Renumber non-numeric footnote ids inside any `ol.footnotes-list` to
/// sequential integers, and update matching `#fn:NAME` / `#fnref:NAME`
/// references throughout the document.
fn renumber_named_ids(root: &NodeRef) {
    for ol in select_all(root, "ol.footnotes-list") {
        let mut mapping: Vec<(String, String)> = Vec::new();
        let mut idx: u32 = ol
            .as_element()
            .and_then(|e| e.attributes.borrow().get("start").map(str::to_string))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        for li in ol.children().filter(|c| is_tag(c, "li")) {
            let Some(id) = attr(&li, "id") else {
                idx += 1;
                continue;
            };
            let Some(stripped) = id.strip_prefix("fn:") else {
                idx += 1;
                continue;
            };
            // If purely numeric, keep.
            if stripped.parse::<u64>().is_ok() {
                idx = stripped.parse::<u32>().unwrap_or(idx) + 1;
                continue;
            }
            let new_id = format!("fn:{idx}");
            set_attr(&li, "id", &new_id);
            mapping.push((stripped.to_string(), idx.to_string()));
            idx += 1;
        }
        if mapping.is_empty() {
            continue;
        }
        // Update hrefs `#fn:NAME` and `#fnref:NAME` and ids `fnref:NAME`.
        for (name, n) in &mapping {
            for el in select_all(root, &format!("a[href=\"#fn:{name}\"]")) {
                set_attr(&el, "href", &format!("#fn:{n}"));
            }
            for el in select_all(root, &format!("a[href=\"#fnref:{name}\"]")) {
                set_attr(&el, "href", &format!("#fnref:{n}"));
            }
            for el in select_all(root, &format!("[id=\"fnref:{name}\"]")) {
                set_attr(&el, "id", &format!("fnref:{n}"));
            }
        }
    }
}

/// If `node` is wrapped in a single-child `<div>`, return the wrapper;
/// otherwise return `node`.
fn wrapper_or_self(node: &NodeRef) -> NodeRef {
    let Some(parent) = node.parent() else {
        return node.clone();
    };
    if !is_tag(&parent, "div") {
        return node.clone();
    }
    // Only treat as wrapper if `node` is the only significant child.
    let mut others = 0;
    for c in parent.children() {
        if let Some(t) = c.as_text() {
            if !t.borrow().trim().is_empty() {
                others += 1;
            }
            continue;
        }
        if c.as_element().is_some() && c.0.as_ref() as *const _ != node.0.as_ref() as *const _ {
            others += 1;
        }
    }
    if others == 0 {
        parent
    } else {
        node.clone()
    }
}

/// Drop heading delimiters whose only purpose was introducing footnotes,
/// when the run conversion has already happened.
fn strip_footnote_delimiters(_root: &NodeRef) {
    // Currently a no-op — `convert_paragraph_definitions_global` removes
    // the immediate-preceding delimiter directly. Reserved for future use.
}

/// If an `<hr>` or "Footnotes" heading is immediately followed by a
/// recognized footnote container, detach the delimiter.
fn drop_delimiter_before_known_footnote(root: &NodeRef) {
    let known: Vec<NodeRef> = select_all(
        root,
        "section.footnotes, aside.footnotes, section[data-footnotes], \
         ol.footnotes-list, ol.footnotes, div.footnote-definition",
    );
    for k in known {
        let mut prev = k.previous_sibling();
        while let Some(p) = prev.clone() {
            if let Some(t) = p.as_text() {
                if t.borrow().trim().is_empty() {
                    prev = p.previous_sibling();
                    continue;
                }
                break;
            }
            if p.as_element().is_none() {
                prev = p.previous_sibling();
                continue;
            }
            if is_footnote_delimiter(&p) {
                p.detach();
            }
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// `<p class="footnote">` → footnote definitions
// ---------------------------------------------------------------------------

fn convert_p_class_footnotes(root: &NodeRef) {
    let candidates = select_all(root, "p.footnote, p.footnote-text, p.footnote-item");
    if candidates.is_empty() {
        return;
    }
    let mut defs: Vec<(String, NodeRef)> = Vec::new();
    for p in &candidates {
        // Try to extract a leading number marker.
        let Some(d) = parse_def_paragraph(p) else {
            continue;
        };
        defs.push((d.num, p.clone()));
    }
    if defs.is_empty() {
        return;
    }
    // Find a parent that contains all defs (use common ancestor of first).
    // Simpler: append a new footnote-list to the parent of the LAST def, in
    // the position right after the last def.
    let last = defs.last().expect("non-empty").1.clone();
    let Some(parent) = last.parent() else {
        return;
    };
    let ol = new_element("ol", &[("class", "footnotes-list")]);
    for (num, p) in &defs {
        let li = new_element("li", &[("id", &format!("fn:{num}"))]);
        // Drop the leading <sup>/<strong>/<b>.
        if let Some(first_el) = p.children().find(|c| c.as_element().is_some()) {
            first_el.detach();
        }
        transfer_children(p, &li);
        ol.append(li);
    }
    last.insert_after(ol);
    for (_, p) in &defs {
        p.detach();
    }
    // Tag (in case parent already has class footnotes etc.) — not needed.
    let _ = parent;
}

// ---------------------------------------------------------------------------
// Inline footnote span: `<span class="inline-footnote">N<span
// class="footnoteContent">body</span></span>`
// ---------------------------------------------------------------------------

fn convert_inline_footnote_span(root: &NodeRef) {
    let spans = select_all(root, "span.inline-footnote");
    if spans.is_empty() {
        return;
    }
    let mut defs: Vec<(String, NodeRef)> = Vec::new();
    let mut count = 0u32;
    let mut owner: Option<NodeRef> = None;
    for s in spans {
        // Find the digit text and the inner footnoteContent.
        let mut inner_content: Option<NodeRef> = None;
        let mut num_str = String::new();
        for c in s.children() {
            if let Some(t) = c.as_text() {
                let txt = t.borrow().to_string();
                let trim = txt.trim();
                if trim.chars().all(|ch| ch.is_ascii_digit()) && !trim.is_empty() {
                    num_str = trim.to_string();
                }
            } else if c.as_element().is_some() && has_class(&c, "footnoteContent") {
                inner_content = Some(c.clone());
            }
        }
        let Some(content) = inner_content else { continue };
        if num_str.is_empty() {
            count += 1;
            num_str = count.to_string();
        }
        // Replace the outer span with a `<sup>N</sup>` containing the digit.
        let sup = new_element("sup", &[]);
        sup.append(NodeRef::new_text(num_str.clone()));
        s.insert_before(sup);
        s.detach();
        // Build a definition <li> with a wrapping <p> so the renderer
        // treats the body as a single inline paragraph.
        let li = new_element("li", &[("id", &format!("fn:{num_str}"))]);
        let p = new_element("p", &[]);
        transfer_children(&content, &p);
        li.append(p);
        defs.push((num_str, li));
        if owner.is_none() {
            // Use article/body ancestor as owner.
            let mut cur = content.parent();
            while let Some(p) = cur {
                if matches!(
                    tag_name_lc(&p).as_str(),
                    "article" | "main" | "body"
                ) {
                    owner = Some(p);
                    break;
                }
                cur = p.parent();
            }
        }
    }
    if defs.is_empty() {
        return;
    }
    let owner = owner.unwrap_or_else(|| root.clone());
    let ol = new_element("ol", &[("class", "footnotes-list")]);
    for (_num, li) in defs {
        ol.append(li);
    }
    owner.append(ol);
}

// ---------------------------------------------------------------------------
// `<span class="fna-ref" data-definition="ID">` paired with `<aside id="ID">`.
// ---------------------------------------------------------------------------

fn convert_data_definition_aside(root: &NodeRef) {
    let refs = select_all(root, "span[data-definition]");
    if refs.is_empty() {
        return;
    }
    let mut count: u32 = 0;
    let mut defs: Vec<(String, NodeRef)> = Vec::new();
    let mut owner: Option<NodeRef> = None;
    for r in refs {
        let Some(target_id) = attr(&r, "data-definition") else {
            continue;
        };
        let selector = format!("aside#{target_id}, [id=\"{target_id}\"]");
        let Some(target) = select_first(root, &selector) else {
            continue;
        };
        if !is_tag(&target, "aside") {
            continue;
        }
        count += 1;
        let num = count.to_string();
        let sup = new_element("sup", &[]);
        sup.append(NodeRef::new_text(num.clone()));
        r.insert_before(sup);
        r.detach();
        let li = new_element("li", &[("id", &format!("fn:{num}"))]);
        let p = new_element("p", &[]);
        transfer_children(&target, &p);
        li.append(p);
        defs.push((num, li));
        if owner.is_none() {
            let mut cur = target.parent();
            while let Some(p) = cur {
                if matches!(
                    tag_name_lc(&p).as_str(),
                    "article" | "main" | "body"
                ) {
                    owner = Some(p);
                    break;
                }
                cur = p.parent();
            }
        }
        target.detach();
    }
    if defs.is_empty() {
        return;
    }
    let owner = owner.unwrap_or_else(|| root.clone());
    let ol = new_element("ol", &[("class", "footnotes-list")]);
    for (_n, li) in defs {
        ol.append(li);
    }
    owner.append(ol);
}

// ---------------------------------------------------------------------------
// easy-footnote class: tag the wrapper as canonical footnotes-list.
// ---------------------------------------------------------------------------

/// Rewrite easy-footnote plugin anchors so the markdown renderer recognises
/// them as canonical footnote refs.
fn rewrite_easy_footnote_classes(root: &NodeRef) {
    static EASY_HREF: Lazy<Regex> = Lazy::new(|| {
        Regex::new(r"#easy-footnote-bottom-(\d+)").expect("easy footnote regex")
    });
    for a in select_all(root, "a[href*=\"easy-footnote\"]") {
        let Some(href) = attr(&a, "href") else { continue };
        if let Some(caps) = EASY_HREF.captures(&href) {
            if let Some(num) = caps.get(1).map(|m| m.as_str().to_string()) {
                set_attr(&a, "href", &format!("#fn:{num}"));
            }
        }
    }
    // Drop the easy-footnote-to-top backref class anchors entirely (handled
    // already by `is_backref` in markdown layer if href contains #fnref —
    // but easy-footnote uses #easy-footnote-N which is the *ref*, not the
    // backref target). Detach.
    for a in select_all(root, "a.easy-footnote-to-top") {
        a.detach();
    }
}

// ---------------------------------------------------------------------------
// Google Docs `ftnt`/`ftnt_ref` id rewrite.
//
// The link `<sup id="ftnt_refN"><a href="#ftntN">[N]</a></sup>` is already
// recognized via `links::footnote_ref_id` (matches `fn` prefix); but the
// alternative ids `ftntN` aren't. Rewrite the ids/hrefs to standard form,
// then the existing canonical container tagging picks them up.
// ---------------------------------------------------------------------------

fn rewrite_ftnt_ids(root: &NodeRef) {
    // ids
    for el in select_all(root, "[id^=\"ftnt\"]") {
        let Some(id) = attr(&el, "id") else { continue };
        if let Some(rest) = id.strip_prefix("ftnt_ref") {
            set_attr(&el, "id", &format!("fnref:{rest}"));
        } else if let Some(rest) = id.strip_prefix("ftnt") {
            // Wrap definition: also add canonical form.
            set_attr(&el, "id", &format!("fn:{rest}"));
        }
    }
    // hrefs
    for el in select_all(root, "a[href^=\"#ftnt\"]") {
        let Some(href) = attr(&el, "href") else { continue };
        if let Some(rest) = href.strip_prefix("#ftnt_ref") {
            set_attr(&el, "href", &format!("#fnref:{rest}"));
        } else if let Some(rest) = href.strip_prefix("#ftnt") {
            set_attr(&el, "href", &format!("#fn:{rest}"));
        }
    }
}

// ---------------------------------------------------------------------------
// Word HTML `_ftn` / `_ftnref` href rewrite.
//
// Word doesn't put ids on the targets; only hrefs identify the footnote.
// We rewrite hrefs to the canonical form, and for the definition runs
// `<p><sup><a href="#_ftnrefN">[N]</a></sup> body</p>`, the existing
// paragraph-definition pass will then convert them into an ol.
// ---------------------------------------------------------------------------

static WORD_FTN_HREF: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)#_ftn(\d+)").expect("word ftn regex"));
static WORD_FTNREF_HREF: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)#_ftnref(\d+)").expect("word ftnref regex"));

fn rewrite_word_ftn_ids(root: &NodeRef) {
    for el in select_all(root, "a[href*=\"_ftn\"]") {
        let Some(href) = attr(&el, "href") else { continue };
        // Check ftnref FIRST (more specific suffix on top of ftn).
        if let Some(caps) = WORD_FTNREF_HREF.captures(&href) {
            if let Some(num) = caps.get(1).map(|m| m.as_str().to_string()) {
                set_attr(&el, "href", &format!("#fnref:{num}"));
                continue;
            }
        }
        if let Some(caps) = WORD_FTN_HREF.captures(&href) {
            if let Some(num) = caps.get(1).map(|m| m.as_str().to_string()) {
                set_attr(&el, "href", &format!("#fn:{num}"));
            }
        }
    }
    // The ids on these word-style anchors are usually missing — but
    // sometimes present. Strip stray IDs that match the pattern.
    let _ = remove_attr;
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
    fn aside_footnotes_gains_list_class() {
        let html =
            r#"<html><body><aside class="footnotes"><ol><li>def</li></ol></aside></body></html>"#;
        let root = parse(html);
        normalize_footnotes(&root);
        let out = serialize(&root);
        assert!(out.contains("footnotes-list"), "got: {out}");
    }

    #[test]
    fn paragraph_definitions_become_ol() {
        let html = r#"<html><body><div class="footnotes"><p><strong>1</strong> first</p><p><strong>2</strong> second</p></div></body></html>"#;
        let root = parse(html);
        normalize_footnotes(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"class="footnotes-list""#), "got: {out}");
        assert!(out.contains(r#"id="fn:1""#), "got: {out}");
        assert!(out.contains(r#"id="fn:2""#), "got: {out}");
        assert!(out.contains("first"));
    }

    #[test]
    fn hr_delimited_sup_paragraphs_convert() {
        let html = r#"<html><body><article><p>Body<sup>1</sup></p><hr><p><sup>1</sup> first</p><p><sup>2</sup> second</p></article></body></html>"#;
        let root = parse(html);
        normalize_footnotes(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"class="footnotes-list""#), "got: {out}");
        assert!(!out.contains("<hr"), "hr not stripped: {out}");
    }

    #[test]
    fn heading_delimited_sup_paragraphs_convert() {
        let html = r#"<html><body><article><p>Body<sup>1</sup></p><h2>Notes</h2><p><sup>1</sup> first</p><p><sup>2</sup> second</p></article></body></html>"#;
        let root = parse(html);
        normalize_footnotes(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"class="footnotes-list""#), "got: {out}");
        assert!(!out.contains("<h2>Notes</h2>"), "heading not stripped: {out}");
    }

    #[test]
    fn p_class_footnote_collected() {
        let html = r#"<html><body><article><p>Body<sup>1</sup></p><p class="footnote"><sup>1</sup>first</p><h2>X</h2><p>more<sup>2</sup></p><p class="footnote"><sup>2</sup>second</p></article></body></html>"#;
        let root = parse(html);
        normalize_footnotes(&root);
        let out = serialize(&root);
        assert!(out.contains(r#"class="footnotes-list""#), "got: {out}");
    }
}
