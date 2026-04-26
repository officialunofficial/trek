//! kuchikiki-based removal passes (DOM-mutating).

use crate::dom::{DomCtx, DomPass};

pub mod content_patterns;
pub mod hidden;
pub mod metadata_block;
pub mod scoring;
pub mod selectors;
pub mod small_images;

#[must_use]
pub fn pre_passes() -> Vec<Box<dyn DomPass>> {
    vec![Box::new(selectors::Selectors), Box::new(hidden::Hidden)]
}

#[must_use]
pub fn post_passes() -> Vec<Box<dyn DomPass>> {
    vec![
        Box::new(small_images::SmallImages),
        Box::new(metadata_block::MetadataBlock),
        Box::new(content_patterns::ContentPatterns),
        Box::new(scoring::Scoring),
    ]
}

pub fn run_pre(root: &kuchikiki::NodeRef, ctx: &DomCtx) {
    for p in pre_passes() {
        #[cfg(feature = "tracing-passes")]
        let before = root.descendants().count();
        p.run(root, ctx);
        #[cfg(feature = "tracing-passes")]
        {
            let after = root.descendants().count();
            eprintln!(
                "[trek-trace] removals::{:<20} nodes {} -> {} ({:+})",
                p.name(),
                before,
                after,
                after as isize - before as isize
            );
        }
    }
}

pub fn run_post(root: &kuchikiki::NodeRef, ctx: &DomCtx) {
    for p in post_passes() {
        #[cfg(feature = "tracing-passes")]
        let before = root.descendants().count();
        p.run(root, ctx);
        #[cfg(feature = "tracing-passes")]
        {
            let after = root.descendants().count();
            eprintln!(
                "[trek-trace] removals::{:<20} nodes {} -> {} ({:+})",
                p.name(),
                before,
                after,
                after as isize - before as isize
            );
        }
    }
}
