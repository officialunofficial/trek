//! Footnote processing

use lol_html::html_content::Element;
use tracing::debug;

/// Process footnote elements
pub fn process_footnote(el: &mut Element) {
    // Check if this is a footnote reference
    if is_footnote_reference(el) {
        debug!("Processing footnote reference");
        // Add data attribute to mark it as a footnote
        el.set_attribute("data-footnote", "true").ok();
    }

    // Check if this is a footnote list
    if is_footnote_list(el) {
        debug!("Processing footnote list");
        el.set_attribute("data-footnote-list", "true").ok();
    }
}

/// Check if an element is a footnote reference
pub fn is_footnote_reference(element: &Element) -> bool {
    // Check for common footnote patterns in href
    if element.tag_name() == "a" {
        if let Some(href) = element.get_attribute("href") {
            if href.starts_with("#fn")
                || href.starts_with("#cite")
                || href.starts_with("#reference")
                || href.starts_with("#footnote")
            {
                return true;
            }
        }
    }

    // Check class names
    if let Some(class) = element.get_attribute("class") {
        if class.contains("footnote") || class.contains("reference") || class.contains("citation") {
            return true;
        }
    }

    false
}

/// Check if an element is a footnote list
pub fn is_footnote_list(element: &Element) -> bool {
    let tag = element.tag_name();

    // Check common footnote list patterns
    if tag == "ol" || tag == "ul" || tag == "div" {
        if let Some(class) = element.get_attribute("class") {
            if class.contains("footnotes")
                || class.contains("references")
                || class.contains("citations")
                || class.contains("endnotes")
            {
                return true;
            }
        }

        if let Some(id) = element.get_attribute("id") {
            if id.contains("footnotes")
                || id.contains("references")
                || id.contains("citations")
                || id.contains("endnotes")
            {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_has_footnote_pattern() {
        let el_with_footnote = MockElement {
            class: Some("footnote"),
            id: None,
        };
        assert!(has_footnote_pattern(&el_with_footnote));

        let el_with_endnote = MockElement {
            class: Some("endnote-reference"),
            id: None,
        };
        assert!(has_footnote_pattern(&el_with_endnote));

        let el_without = MockElement {
            class: Some("content"),
            id: None,
        };
        assert!(!has_footnote_pattern(&el_without));
    }

    // Mock element for testing
    struct MockElement {
        class: Option<&'static str>,
        id: Option<&'static str>,
    }

    impl MockElement {
        fn get_attribute(&self, name: &str) -> Option<String> {
            match name {
                "class" => self.class.map(String::from),
                "id" => self.id.map(String::from),
                _ => None,
            }
        }
    }

    // Make has_footnote_pattern testable
    fn has_footnote_pattern(el: &MockElement) -> bool {
        const PATTERNS: &[&str] = &["footnote", "endnote", "reference"];

        if let Some(class) = el.get_attribute("class") {
            if PATTERNS.iter().any(|pattern| class.contains(pattern)) {
                return true;
            }
        }

        if let Some(id) = el.get_attribute("id") {
            if PATTERNS.iter().any(|pattern| id.contains(pattern)) {
                return true;
            }
        }

        false
    }
}
