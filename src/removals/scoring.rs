//! Score-based removal — light port of Defuddle's `ContentScorer`.
//!
//! Score blocks below a threshold (low text + many links) and remove them.

use kuchikiki::NodeRef;

use crate::constants::NAVIGATION_INDICATORS;
use crate::dom::walk::{
    count_words, descendants_post_order, element_children, get_attr, is_any_tag, link_text_length,
    text_content,
};
use crate::dom::{DomCtx, DomPass};

pub struct Scoring;

fn score_block(node: &NodeRef) -> i32 {
    let txt = text_content(node);
    let txt_len = txt.len();
    if txt_len == 0 {
        return -50;
    }
    let words = count_words(&txt) as i32;
    let mut score = words / 5;

    // Boost for paragraph children.
    let paragraphs = element_children(node)
        .iter()
        .filter(|c| is_any_tag(c, &["p"]))
        .count() as i32;
    score += paragraphs * 5;

    // Penalty: link density.
    let link_len = link_text_length(node) as i32;
    let density = link_len as f64 / txt_len.max(1) as f64;
    if density > 0.6 {
        score -= 25;
    }
    if density > 0.8 {
        score -= 20;
    }

    // Penalty: navigation indicator words in classes/text.
    let mut nav_hits = 0;
    if let Some(class) = get_attr(node, "class") {
        let lc = class.to_ascii_lowercase();
        for kw in NAVIGATION_INDICATORS {
            if lc.contains(kw) {
                nav_hits += 1;
            }
        }
    }
    score -= nav_hits * 5;

    score
}

fn looks_like_real_content(node: &NodeRef) -> bool {
    // Anything containing real content tags is preserved.
    if node
        .descendants()
        .any(|d| is_any_tag(&d, &["pre", "table", "figure", "picture", "blockquote"]))
    {
        return true;
    }
    // Multiple paragraphs with prose.
    let paragraphs: Vec<NodeRef> = node
        .descendants()
        .filter(|d| is_any_tag(d, &["p"]))
        .collect();
    if paragraphs.len() >= 2 {
        let mut prose_count = 0;
        for p in paragraphs {
            if count_words(&text_content(&p)) >= 10 {
                prose_count += 1;
            }
        }
        if prose_count >= 2 {
            return true;
        }
    }
    false
}

impl DomPass for Scoring {
    fn name(&self) -> &'static str {
        "scoring"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        let mut to_remove: Vec<NodeRef> = Vec::new();
        for d in descendants_post_order(root) {
            if d.parent().is_none() {
                continue;
            }
            if !is_any_tag(&d, &["div", "section", "aside"]) {
                continue;
            }
            if is_any_tag(&d, &["html", "body", "head"]) {
                continue;
            }
            if looks_like_real_content(&d) {
                continue;
            }
            // Only consider blocks with at least some text.
            let txt = text_content(&d);
            let words = count_words(&txt);
            if words < 4 {
                continue;
            }
            let score = score_block(&d);
            // Heavily link-dense, low-content block → drop.
            if score < -10 {
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
