//! kuchikiki-based standardize passes.
//!
//! Each submodule implements a single Defuddle-equivalent transformation as
//! a `DomPass`. The orchestration glue lives in `crate::lib::run_dom_passes`.

use crate::dom::{DomCtx, DomPass};

pub mod code_blocks;
pub mod figure_image;
pub mod flatten_wrappers;
pub mod footnotes;
pub mod headings;
pub mod promote_semantics;
pub mod relative_urls;
pub mod tables;

/// Re-export the existing legacy `standardize_content` so callers in
/// `lib.rs` keep their import paths. The new DOM passes run *after* this
/// legacy text-level pass.
pub use crate::standardize_legacy::standardize_content;

/// Build the default sequence of standardize DOM passes (in order).
///
/// `TREK_C_SKIP=name1,name2` env var disables specific passes (matching
/// `DomPass::name`). Track-D's `elements::normalize_all` already handles
/// most code-block normalisation, so `code_blocks` is left in the default
/// list but conservatively scoped.
#[must_use]
pub fn default_passes() -> Vec<Box<dyn DomPass>> {
    let skip = std::env::var("TREK_C_SKIP").unwrap_or_default();
    let skip_set: Vec<String> = skip
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let all: Vec<Box<dyn DomPass>> = vec![
        Box::new(code_blocks::CodeBlocks),
        Box::new(flatten_wrappers::FlattenWrappers),
        Box::new(promote_semantics::PromoteSemantics),
        Box::new(relative_urls::RelativeUrls),
        Box::new(figure_image::FigureImage),
        Box::new(footnotes::Footnotes),
        Box::new(tables::Tables),
        Box::new(headings::Headings),
    ];
    all.into_iter()
        .filter(|p| !skip_set.iter().any(|s| s == p.name()))
        .collect()
}

/// Run all standardize DOM passes in order against `root`.
pub fn run_all(root: &kuchikiki::NodeRef, ctx: &DomCtx) {
    for p in default_passes() {
        #[cfg(feature = "tracing-passes")]
        let before = root.descendants().count();
        p.run(root, ctx);
        #[cfg(feature = "tracing-passes")]
        {
            let after = root.descendants().count();
            eprintln!(
                "[trek-trace] standardize::{:<20} nodes {} -> {} ({:+})",
                p.name(),
                before,
                after,
                after as isize - before as isize
            );
        }
    }
}
