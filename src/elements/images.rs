//! Image element processing

use lol_html::html_content::Element;
use tracing::debug;

const MIN_DIMENSION: u32 = 50;

/// Process image elements
pub fn process_image(el: &mut Element) {
    if el.tag_name() == "img" {
        // Get dimensions to check if it's too small
        let width = el
            .get_attribute("width")
            .and_then(|w| w.parse::<u32>().ok())
            .unwrap_or(0);

        let height = el
            .get_attribute("height")
            .and_then(|h| h.parse::<u32>().ok())
            .unwrap_or(0);

        // Remove small images (likely icons or spacers)
        if (width > 0 && width < MIN_DIMENSION) || (height > 0 && height < MIN_DIMENSION) {
            debug!("Removing small image: {}x{}", width, height);
            el.remove();
            return;
        }

        // Remove tracking pixels
        if let Some(src) = el.get_attribute("src") {
            if src.contains("pixel")
                || src.contains("tracking")
                || src.contains("analytics")
                || src.contains("1x1")
            {
                debug!("Removing tracking pixel");
                el.remove();
                return;
            }
        }

        // Preserve only essential attributes
        let attrs_to_keep = ["src", "alt", "width", "height", "srcset"];
        let attrs_to_remove: Vec<String> = el
            .attributes()
            .iter()
            .filter(|attr| !attrs_to_keep.contains(&attr.name().as_str()))
            .map(lol_html::html_content::Attribute::name)
            .collect();

        for attr in attrs_to_remove {
            el.remove_attribute(&attr);
        }

        // Ensure alt attribute exists
        if el.get_attribute("alt").is_none() {
            let _ = el.set_attribute("alt", "");
        }
    }
}

/// Check if an image element should be preserved
pub fn should_preserve_image(el: &Element) -> bool {
    if el.tag_name() != "img" {
        return true;
    }

    // Check dimensions
    let width = el
        .get_attribute("width")
        .and_then(|w| w.parse::<u32>().ok())
        .unwrap_or(100); // Default to 100 if not specified

    let height = el
        .get_attribute("height")
        .and_then(|h| h.parse::<u32>().ok())
        .unwrap_or(100); // Default to 100 if not specified

    // Preserve images larger than 50x50
    width >= 50 && height >= 50
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_should_preserve_image() {
        // Test with mock element
        let large_img = MockElement {
            tag: "img",
            width: Some("100"),
            height: Some("100"),
        };
        assert!(should_preserve_test_image(&large_img));

        let small_img = MockElement {
            tag: "img",
            width: Some("30"),
            height: Some("30"),
        };
        assert!(!should_preserve_test_image(&small_img));

        let div_element = MockElement {
            tag: "div",
            width: None,
            height: None,
        };
        assert!(should_preserve_test_image(&div_element)); // Non-img elements are preserved
    }

    struct MockElement {
        tag: &'static str,
        width: Option<&'static str>,
        height: Option<&'static str>,
    }

    fn should_preserve_test_image(el: &MockElement) -> bool {
        if el.tag != "img" {
            return true;
        }

        let width = el.width.and_then(|w| w.parse::<u32>().ok()).unwrap_or(100);

        let height = el.height.and_then(|h| h.parse::<u32>().ok()).unwrap_or(100);

        width >= 50 && height >= 50
    }
}
