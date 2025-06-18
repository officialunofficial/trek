//! Code block processing

use lol_html::html_content::Element;

/// Process code elements
pub fn process_code_element(el: &mut Element) {
    let tag_name = el.tag_name();

    if tag_name == "pre" || tag_name == "code" {
        // Extract language from class if present
        if let Some(class_attr) = el.get_attribute("class") {
            let has_language = class_attr
                .split_whitespace()
                .any(|class| class.starts_with("language-"));

            if !has_language && tag_name == "code" {
                // If code block doesn't have a language class, check parent
                // Note: lol_html doesn't provide parent access, so we'll just preserve the element
            }
        }

        // Remove unwanted attributes but keep class for language info
        let class_value = el.get_attribute("class");
        let attrs_to_remove: Vec<String> = el
            .attributes()
            .iter()
            .filter(|attr| attr.name() != "class")
            .map(lol_html::html_content::Attribute::name)
            .collect();

        for attr in attrs_to_remove {
            el.remove_attribute(&attr);
        }

        // Re-add class if it had language info
        if let Some(class) = class_value {
            if class.split_whitespace().any(|c| c.starts_with("language-")) {
                let _ = el.set_attribute("class", &class);
            }
        }
    }
}

/// Standardize code blocks to a consistent format
pub fn standardize_code_block(html: &str) -> String {
    // This is a simplified version - in production, you'd use lol_html
    html.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standardize_code_block() {
        let html = r#"<pre><code class="language-rust">fn main() {}</code></pre>"#;
        let result = standardize_code_block(html);
        assert!(result.contains("language-rust"));
    }
}
