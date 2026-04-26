//! High-impact content-pattern removal heuristics ported from
//! Defuddle's `removeByContentPattern`.
//!
//! We pick the 8-12 highest-impact heuristics from the 29-item list and
//! leave the rest as TODO (see TODO.md in this directory's parent).

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::dom::walk::{
    count_words, descendants_post_order, descendants_pre_order, element_children, get_attr,
    is_any_tag, link_text_length, tag_name, text_content,
};
use crate::dom::{DomCtx, DomPass};

pub struct ContentPatterns;

static SOCIAL_COUNT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*\d+\s+(?:like|likes|comment|comments|reply|replies|share|shares)\s*$")
        .expect("valid regex")
});

static READ_TIME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*\d+\s*(?:min|minute|minutes)\s+read\s*$").expect("valid regex")
});

static BYLINE_BY_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*by\s+[A-Z][\w'\-]+(?:\s+[A-Z][\w'\-]+)*").expect("valid regex")
});

static SHARE_FOLLOW_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(?:share|follow|tweet|like|subscribe|sign up|sign in|copy link)\s*$")
        .expect("valid regex")
});

static NEWSLETTER_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\b(?:subscribe to (?:our|the) newsletter|join (?:our|the) newsletter|sign up for (?:our|the) newsletter|email(?: address)? to subscribe|never miss a (?:post|story))\b").expect("valid regex")
});

fn block_words(node: &NodeRef) -> usize {
    count_words(&text_content(node))
}

fn link_density(node: &NodeRef) -> f64 {
    let txt_len = text_content(node).len();
    if txt_len == 0 {
        return 0.0;
    }
    link_text_length(node) as f64 / txt_len as f64
}

fn drop_social_counters(root: &NodeRef) {
    // Match elements whose entire text is "13 Likes" / "3 Comments" etc.
    for d in descendants_post_order(root) {
        if d.parent().is_none() {
            continue;
        }
        let Some(name) = tag_name(&d) else { continue };
        if !matches!(name.as_str(), "div" | "span" | "p" | "li" | "a") {
            continue;
        }
        let txt = text_content(&d);
        if SOCIAL_COUNT_RE.is_match(&txt) {
            // Walk up to a wrapper if the parent's only meaningful child is `d`.
            let mut target = d.clone();
            while let Some(parent) = target.parent() {
                if parent.as_element().is_none() {
                    break;
                }
                let kids = element_children(&parent);
                if kids.len() == 1 && block_words(&parent) == count_words(&txt) {
                    target = parent.clone();
                } else {
                    break;
                }
            }
            target.detach();
        }
    }
}

fn drop_read_time(root: &NodeRef) {
    for d in descendants_post_order(root) {
        if d.parent().is_none() {
            continue;
        }
        let Some(name) = tag_name(&d) else { continue };
        if !matches!(name.as_str(), "div" | "span" | "p" | "li") {
            continue;
        }
        let txt = text_content(&d);
        if READ_TIME_RE.is_match(&txt) {
            d.detach();
        }
    }
}

fn drop_share_follow_widgets(root: &NodeRef) {
    for d in descendants_post_order(root) {
        if d.parent().is_none() {
            continue;
        }
        let Some(name) = tag_name(&d) else { continue };
        if !matches!(name.as_str(), "div" | "section" | "ul" | "p") {
            continue;
        }
        let txt = text_content(&d).trim().to_string();
        if txt.is_empty() {
            continue;
        }
        let words = count_words(&txt);
        if words > 8 {
            continue;
        }
        if SHARE_FOLLOW_RE.is_match(&txt) {
            d.detach();
        }
    }
}

fn drop_newsletter_signups(root: &NodeRef) {
    for d in descendants_post_order(root) {
        if d.parent().is_none() {
            continue;
        }
        let Some(name) = tag_name(&d) else { continue };
        if !matches!(name.as_str(), "div" | "section" | "aside" | "form" | "ul") {
            continue;
        }
        let txt = text_content(&d);
        if NEWSLETTER_RE.is_match(&txt) {
            d.detach();
        }
    }
}

fn drop_breadcrumb_at_start(root: &NodeRef) {
    // Find the most-meaningful start container — the deepest body/main/article
    // whose direct children form the article-body sequence. Walk down through
    // single-child wrappers so we look at the actual leading content.
    let body = root.descendants().find(|d| is_any_tag(d, &["body"]));
    let mut scope = body.unwrap_or_else(|| root.clone());
    for _ in 0..6 {
        let kids = element_children(&scope);
        if kids.len() == 1 && is_any_tag(&kids[0], &["main", "article", "div", "section"]) {
            scope = kids[0].clone();
        } else {
            break;
        }
    }
    // Strip leading orphan HR/BR (the legacy text-level standardiser leaves
    // these behind once a metadata block above the article body has been
    // removed). Walk forward until the first real content element.
    loop {
        let kids = element_children(&scope);
        if let Some(first) = kids.first() {
            if is_any_tag(first, &["hr", "br"]) {
                first.detach();
                continue;
            }
        }
        break;
    }
    // Also strip an HR/BR sitting between the H1 and the first prose
    // paragraph — common after metadata-block removals carve out a
    // <div class="meta"> sibling that originally separated the heading
    // from the body.
    let kids = element_children(&scope);
    if kids.len() >= 2 {
        if is_any_tag(&kids[0], &["h1"]) && is_any_tag(&kids[1], &["hr", "br"]) {
            kids[1].detach();
        }
    }
    let kids = element_children(&scope);
    for k in kids.into_iter().take(4) {
        if k.parent().is_none() {
            continue;
        }
        // If we reach a heading or a real prose paragraph, stop.
        if is_any_tag(&k, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
            break;
        }
        // Direct UL/OL/NAV breadcrumb.
        if is_any_tag(&k, &["ul", "ol", "nav"]) {
            if looks_like_breadcrumb_list(&k) {
                k.detach();
                continue;
            }
        }
        // Wrapper div/section/aside containing exactly one breadcrumb-shaped UL/OL/NAV.
        if is_any_tag(&k, &["div", "section", "aside"]) {
            let inner = element_children(&k);
            if inner.len() == 1 && is_any_tag(&inner[0], &["ul", "ol", "nav"]) {
                if looks_like_breadcrumb_list(&inner[0]) {
                    k.detach();
                    continue;
                }
            }
        }
        // Stop walking past the first long-prose paragraph.
        let txt = text_content(&k);
        let words = count_words(txt.trim());
        if words >= 12 && is_any_tag(&k, &["p", "div", "section", "blockquote"]) {
            break;
        }
    }
}

fn looks_like_breadcrumb_list(node: &NodeRef) -> bool {
    let txt = text_content(node);
    let trimmed = txt.trim();
    let sep_count = trimmed
        .matches(|c: char| c == '/' || c == '>' || c == '·' || c == '|' || c == '\u{203A}')
        .count();
    let words = count_words(trimmed);
    let li_count = node
        .descendants()
        .filter(|d| is_any_tag(d, &["li"]))
        .count();
    let a_count = node
        .descendants()
        .filter(|d| is_any_tag(d, &["a"]))
        .count();
    if words >= 25 {
        return false;
    }
    if sep_count >= 2 {
        return true;
    }
    // 2-6 short list items, most/all bearing links.
    if (2..=6).contains(&li_count) && a_count >= li_count.saturating_sub(1) {
        return true;
    }
    false
}

fn drop_trailing_related_links(root: &NodeRef) {
    // Walk last few children; if a heading + ul of links pattern is found
    // matching "Related", "Read next", "More from", drop them.
    let body = root.descendants().find(|d| is_any_tag(d, &["body"]));
    let scope = body.unwrap_or_else(|| root.clone());
    let kids = element_children(&scope);
    if kids.is_empty() {
        return;
    }
    let n = kids.len();
    for idx in (n.saturating_sub(6)..n).rev() {
        let k = &kids[idx];
        if k.parent().is_none() {
            continue;
        }
        let txt = text_content(k);
        let txt_lc = txt.to_ascii_lowercase();
        if is_any_tag(k, &["h1", "h2", "h3", "h4", "h5", "h6"])
            && (txt_lc.contains("related")
                || txt_lc.contains("read next")
                || txt_lc.contains("more from")
                || txt_lc.contains("further reading")
                || txt_lc.contains("about the author")
                || txt_lc.starts_with("comments"))
        {
            // Detach this heading and everything after it.
            for j in idx..n {
                if let Some(node) = kids.get(j) {
                    if node.parent().is_some() {
                        node.detach();
                    }
                }
            }
            return;
        }
    }
}

fn drop_link_dense_trailing(root: &NodeRef) {
    // Walk backwards through trailing children, skipping bare HR/BR.
    // For each candidate container (div/section/ul/aside), drop if its
    // link density looks like a related-posts/tag block.
    let body = root.descendants().find(|d| is_any_tag(d, &["body"]));
    let mut scope = body.unwrap_or_else(|| root.clone());
    for _ in 0..6 {
        let kids = element_children(&scope);
        if kids.len() == 1 && is_any_tag(&kids[0], &["main", "article", "div", "section"]) {
            scope = kids[0].clone();
        } else {
            break;
        }
    }
    let kids = element_children(&scope);
    if kids.is_empty() {
        return;
    }
    let n = kids.len();
    for idx in (0..n).rev() {
        let k = kids[idx].clone();
        if k.parent().is_none() {
            continue;
        }
        // Skip horizontal rules / bare line breaks at the very end.
        if is_any_tag(&k, &["hr", "br"]) {
            k.detach();
            continue;
        }
        if !is_any_tag(&k, &["div", "section", "ul", "aside", "p"]) {
            break;
        }
        let words = block_words(&k);
        if words < 6 {
            // Empty/near-empty trailing wrapper — drop it.
            if words == 0 {
                k.detach();
                continue;
            }
            break;
        }
        if link_density(&k) > 0.6 && words < 200 {
            k.detach();
            continue;
        }
        // Preserve real content.
        break;
    }
}

fn drop_trailing_author_block(root: &NodeRef) {
    // Walk last few children of the deepest single-child wrapper. Drop
    // trailing blocks whose visible text matches "By X" / "March 4, 2026"
    // / similar short author/date patterns. Stops at the first long block.
    let body = root.descendants().find(|d| is_any_tag(d, &["body"]));
    let mut scope = body.unwrap_or_else(|| root.clone());
    for _ in 0..6 {
        let kids = element_children(&scope);
        if kids.len() == 1 && is_any_tag(&kids[0], &["main", "article", "div", "section"]) {
            scope = kids[0].clone();
        } else {
            break;
        }
    }
    let kids = element_children(&scope);
    if kids.is_empty() {
        return;
    }
    let n = kids.len();
    // Walk last ~6 elements from the back. Drop short byline/date blocks.
    let start = n.saturating_sub(6);
    for idx in (start..n).rev() {
        let k = kids[idx].clone();
        if k.parent().is_none() {
            continue;
        }
        let txt = text_content(&k);
        let trimmed = txt.trim();
        let words = count_words(trimmed);
        if words > 30 {
            // Real prose — stop walking up.
            break;
        }
        if words == 0 {
            continue;
        }
        if is_any_tag(&k, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
            continue;
        }
        let by_match = BYLINE_BY_RE.is_match(trimmed);
        let date_match = once_cell::sync::Lazy::force(&TRAILING_DATE_RE).is_match(trimmed);
        let lc = trimmed.to_ascii_lowercase();
        let label_match = lc.starts_with("posted in")
            || lc.starts_with("filed under")
            || lc.starts_with("tags ")
            || lc == "tags"
            || lc.starts_with("tagged ");
        let mostly_meta = (by_match || date_match || label_match) && words < 14;
        if mostly_meta {
            k.detach();
            continue;
        }
        // Container whose only meaningful descendants are byline + date paragraphs.
        if is_any_tag(&k, &["section", "div", "aside"]) {
            let inner = text_content(&k);
            let inner_t = inner.trim();
            let inner_w = count_words(inner_t);
            if inner_w < 14 {
                let has_by = BYLINE_BY_RE.is_match(inner_t);
                let has_date = once_cell::sync::Lazy::force(&TRAILING_DATE_RE).is_match(inner_t);
                if has_by || has_date {
                    k.detach();
                    continue;
                }
            }
        }
    }
}

static TRAILING_DATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s*\d{0,4}|\d{4}[-/]\d{1,2}[-/]\d{1,2}").expect("valid regex")
});

fn drop_pinned_or_label_widgets(root: &NodeRef) {
    // Strip elements whose only text matches a known stand-alone label.
    let labels = [
        "share this article",
        "share this story",
        "pinned",
        "advertisement",
        "loading\u{2026}",
        "loading...",
        "loading",
        "more like this",
        "table of contents",
        "categories",
        "tags",
    ];
    for d in descendants_post_order(root) {
        if d.parent().is_none() {
            continue;
        }
        let Some(name) = tag_name(&d) else { continue };
        if !matches!(name.as_str(), "div" | "section" | "p" | "span" | "ul") {
            continue;
        }
        let txt = text_content(&d).trim().to_ascii_lowercase();
        if txt.is_empty() {
            continue;
        }
        if labels.iter().any(|l| txt == *l) {
            d.detach();
        }
    }
}


fn drop_byline_near_start(root: &NodeRef) {
    // Find first H1.
    let h1 = descendants_pre_order(root)
        .into_iter()
        .find(|d| is_any_tag(d, &["h1"]));
    let Some(h1) = h1 else { return };
    // Look at the next ~6 siblings/text-nodes after H1 and drop short
    // "byline-ish" blocks: `By Author`, ISO/long-form dates, `N min read`,
    // single-author author-list ULs, and "Posted in"/"Filed under" labels.
    let mut cur = h1.next_sibling();
    let mut count = 0;
    while let Some(s) = cur {
        if let Some(t) = s.as_text() {
            let txt = t.borrow().to_string();
            let trimmed = txt.trim();
            if trimmed.is_empty() {
                cur = s.next_sibling();
                continue;
            }
            count += 1;
            if count > 6 {
                break;
            }
            let words = count_words(trimmed);
            let is_short = words < 12;
            if is_short
                && (BYLINE_BY_RE.is_match(trimmed)
                    || ISO_DATE_RE.is_match(trimmed)
                    || LONG_DATE_RE.is_match(trimmed)
                    || READ_TIME_RE.is_match(trimmed))
            {
                let next = s.next_sibling();
                s.detach();
                cur = next;
                continue;
            }
            if words >= 12 {
                break;
            }
            cur = s.next_sibling();
            continue;
        }
        if s.as_element().is_some() {
            count += 1;
            if count > 6 {
                break;
            }
            let txt = text_content(&s);
            let trimmed = txt.trim();
            if trimmed.is_empty() {
                cur = s.next_sibling();
                continue;
            }
            let words = count_words(trimmed);
            let is_short = words < 12;
            let is_byline = BYLINE_BY_RE.is_match(trimmed);
            let is_iso_date = is_short && ISO_DATE_RE.is_match(trimmed);
            let is_long_date = is_short && LONG_DATE_RE.is_match(trimmed);
            let is_author_list = is_short
                && is_any_tag(&s, &["ul", "ol"])
                && get_attr(&s, "class")
                    .map(|c| c.to_ascii_lowercase().contains("author"))
                    .unwrap_or(false);
            let is_read_time = READ_TIME_RE.is_match(trimmed);
            if is_byline || is_iso_date || is_long_date || is_author_list || is_read_time {
                let next = s.next_sibling();
                s.detach();
                cur = next;
                continue;
            }
            // Stop walking past the first long-prose paragraph.
            if words >= 12 && is_any_tag(&s, &["p", "div", "section", "blockquote"]) {
                break;
            }
        }
        cur = s.next_sibling();
    }
}

static ISO_DATE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*\d{4}-\d{1,2}-\d{1,2}\s*$").expect("valid regex"));

static LONG_DATE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*(?:\d{1,2}\s+)?(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2},?\s*\d{0,4}\s*$").expect("valid regex")
});

impl DomPass for ContentPatterns {
    fn name(&self) -> &'static str {
        "content_patterns"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        drop_social_counters(root);
        drop_read_time(root);
        drop_share_follow_widgets(root);
        drop_newsletter_signups(root);
        drop_breadcrumb_at_start(root);
        drop_trailing_related_links(root);
        drop_link_dense_trailing(root);
        drop_pinned_or_label_widgets(root);
        drop_byline_near_start(root);
        drop_trailing_author_block(root);
    }
}
