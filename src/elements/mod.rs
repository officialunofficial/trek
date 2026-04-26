//! Element processors for different content types.
//!
//! Track D — kuchikiki-tree element handlers, ported from Defuddle's
//! `src/elements/`. Each public `normalize_*` function mutates the DOM in
//! place; `normalize_all` runs them in pipeline order.

pub mod callouts;
pub mod code;
pub mod footnotes;
pub mod headings;
pub mod images;
pub mod math;
pub(crate) mod util;

pub use callouts::normalize_callouts;
pub use code::normalize_code_blocks;
pub use footnotes::normalize_footnotes;
pub use headings::normalize_headings;
pub use images::normalize_images;
#[cfg(feature = "math-base")]
pub use math::normalize_math_base;

use kuchikiki::NodeRef;
use lol_html::html_content::Element;

/// Run every Track-D element normalization pass against `root` in pipeline
/// order: callouts → math → images → code → headings → footnotes.
///
/// This is the single function the main pipeline calls after standardize
/// and before markdown rendering.
pub fn normalize_all(root: &NodeRef) {
    normalize_callouts(root);
    #[cfg(feature = "math-base")]
    normalize_math_base(root);
    normalize_images(root);
    normalize_code_blocks(root);
    normalize_headings(root);
    normalize_footnotes(root);
}

/// Legacy lol_html-based trait kept for backwards compatibility with
/// pre-Track-D callers. New element passes use the kuchikiki helpers above.
pub trait ElementProcessor {
    /// Process an element
    fn process(element: &mut Element);

    /// Check if this processor can handle the given element
    fn can_process(element: &Element) -> bool;
}
