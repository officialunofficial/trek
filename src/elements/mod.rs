//! Element processors for different content types

pub mod code;
pub mod footnotes;
pub mod headings;
pub mod images;

use lol_html::html_content::Element;

/// Trait for element processors
pub trait ElementProcessor {
    /// Process an element
    fn process(element: &mut Element);

    /// Check if this processor can handle the given element
    fn can_process(element: &Element) -> bool;
}
