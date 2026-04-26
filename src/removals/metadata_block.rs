//! Remove a date-bearing metadata block sibling near the article H1.
//!
//! Looks at siblings within 3 elements of the title H1 and removes a single
//! candidate that contains a date pattern but no significant prose. Helps
//! match Defuddle output where the byline/published lump is consumed
//! into metadata rather than re-rendered in the body.

use kuchikiki::NodeRef;
use once_cell::sync::Lazy;
use regex::Regex;

use crate::dom::walk::{count_words, descendants_pre_order, is_any_tag, text_content};
use crate::dom::{DomCtx, DomPass};

pub struct MetadataBlock;

static DATE_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]*\s+\d{1,2}|\d{1,2}\s+(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)|\d{4}[-/]\d{1,2}[-/]\d{1,2}").expect("valid regex")
});

static BYLINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)\bby\s+[A-Z]").expect("valid regex"));

impl DomPass for MetadataBlock {
    fn name(&self) -> &'static str {
        "metadata_block"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        // Find first H1.
        let h1 = descendants_pre_order(root)
            .into_iter()
            .find(|d| is_any_tag(d, &["h1"]));
        let Some(h1) = h1 else { return };

        // Walk up to 3 siblings forward / backward; remove a date-ish block
        // with low word count.
        let mut to_check: Vec<NodeRef> = Vec::new();
        let mut cur = h1.next_sibling();
        let mut count = 0;
        while let Some(s) = cur {
            if s.as_element().is_some() {
                to_check.push(s.clone());
                count += 1;
                if count >= 3 {
                    break;
                }
            }
            cur = s.next_sibling();
        }
        let mut cur = h1.previous_sibling();
        let mut count = 0;
        while let Some(s) = cur {
            if s.as_element().is_some() {
                to_check.push(s.clone());
                count += 1;
                if count >= 3 {
                    break;
                }
            }
            cur = s.previous_sibling();
        }

        for n in to_check {
            let txt = text_content(&n);
            let words = count_words(&txt);
            if words > 20 {
                continue;
            }
            // Only remove if it contains a date or byline pattern.
            if DATE_PATTERN.is_match(&txt) || BYLINE_RE.is_match(&txt) {
                // Don't remove headings.
                if is_any_tag(&n, &["h1", "h2", "h3", "h4", "h5", "h6"]) {
                    continue;
                }
                if n.parent().is_some() {
                    n.detach();
                }
            }
        }
    }
}
