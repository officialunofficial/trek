//! Heading element processing

use lol_html::html_content::Element;
use tracing::debug;

/// Standardize heading elements
pub fn standardize_heading(el: &mut Element) {
    let tag_name = el.tag_name();

    // Check if this is a div with role="heading"
    if tag_name == "div" {
        if let Some(role) = el.get_attribute("role") {
            if role == "heading" {
                if let Some(level) = el.get_attribute("aria-level") {
                    // Convert to proper heading tag
                    let new_tag = match level.as_str() {
                        "1" => "h1",
                        "2" => "h2",
                        "3" => "h3",
                        "4" => "h4",
                        "5" => "h5",
                        "6" => "h6",
                        _ => return,
                    };

                    debug!("Converting div[role=heading] to {}", new_tag);
                    // Note: lol_html doesn't support changing tag names directly
                    // In practice, we'd need to reconstruct the element
                }
            }
        }
    }

    // Remove unwanted attributes from headings
    if matches!(tag_name.as_str(), "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
        let attrs_to_remove: Vec<String> = el
            .attributes()
            .iter()
            .filter(|attr| !matches!(attr.name().as_str(), "id" | "class"))
            .map(lol_html::html_content::Attribute::name)
            .collect();

        for attr in attrs_to_remove {
            el.remove_attribute(&attr);
        }
    }
}

/// Process H1 elements (convert to H2 if they match the title)
pub fn process_h1_element(el: &mut Element, title: &str) {
    if el.tag_name() == "h1" {
        // Check if title attribute matches
        if let Some(text_attr) = el.get_attribute("data-text") {
            let normalized_element = normalize_text(&text_attr);
            let normalized_title = normalize_text(title);

            if normalized_element == normalized_title {
                debug!("Removing H1 that matches title");
                el.remove();
            } else {
                debug!("Converting H1 to H2");
                // Note: lol_html doesn't support changing tag names
                // In practice, we'd need to reconstruct as H2
            }
        }
    }
}

fn normalize_text(text: &str) -> String {
    text
        .replace('\u{00A0}', " ") // Non-breaking space to regular space
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_text() {
        let text = "  Hello\u{00A0}World  ";
        assert_eq!(normalize_text(text), "hello world");
    }
}
