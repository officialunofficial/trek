//! Convert HTML to readable plain text while preserving structure

use lol_html::{RewriteStrSettings, element, rewrite_str, text};
use std::sync::{Arc, Mutex};

/// Remove script, style, and noscript elements from HTML
#[allow(clippy::disallowed_methods)] // lol_html macros use unwrap internally
fn remove_skip_elements(html: &str) -> String {
    let settings = RewriteStrSettings {
        element_content_handlers: vec![element!("script, style, noscript", |el| {
            el.remove();
            Ok(())
        })],
        ..RewriteStrSettings::default()
    };

    rewrite_str(html, settings).unwrap_or_else(|_| html.to_string())
}

/// Convert HTML to plain text while preserving readability and structure
#[allow(clippy::disallowed_methods)] // lol_html macros use unwrap internally
pub fn html_to_text(html: &str) -> String {
    // First pass: remove script, style, and noscript elements
    let cleaned_html = remove_skip_elements(html);

    let text_content = Arc::new(Mutex::new(String::new()));
    let text_clone = Arc::clone(&text_content);
    let text_clone2 = Arc::clone(&text_content);
    let text_clone3 = Arc::clone(&text_content);
    let text_clone4 = Arc::clone(&text_content);
    let text_clone5 = Arc::clone(&text_content);
    let text_clone6 = Arc::clone(&text_content);

    let settings = RewriteStrSettings {
        element_content_handlers: vec![
            // Handle line breaks
            element!("br", move |_el| {
                let mut text = text_clone.lock().unwrap();
                text.push('\n');
                Ok(())
            }),
            // Handle paragraphs and divs - add newlines
            element!("p, div, article, section, blockquote", move |el| {
                let mut text = text_clone2.lock().unwrap();
                // Add newline before if content exists and doesn't end with newline
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }

                el.after("\n", lol_html::html_content::ContentType::Text);
                Ok(())
            }),
            // Handle headings - add newlines
            element!("h1, h2, h3, h4, h5, h6", move |el| {
                let mut text = text_clone3.lock().unwrap();
                // Add newline before if content exists
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }

                el.after("\n\n", lol_html::html_content::ContentType::Text);
                Ok(())
            }),
            // Handle list items
            element!("li", move |el| {
                let mut text = text_clone4.lock().unwrap();
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str("• ");

                el.after("\n", lol_html::html_content::ContentType::Text);
                Ok(())
            }),
            // Handle images - add alt text if available
            element!("img", move |el| {
                if let Some(alt) = el.get_attribute("alt") {
                    if !alt.trim().is_empty() {
                        let mut text = text_clone6.lock().unwrap();
                        use std::fmt::Write;
                        let _ = write!(text, " [Image: {}] ", alt.trim());
                    }
                }
                Ok(())
            }),
            // Handle horizontal rules
            element!("hr", move |el| {
                el.replace("\n---\n", lol_html::html_content::ContentType::Text);
                Ok(())
            }),
            // Collect text content from all other elements
            text!("*", move |t| {
                let content = t.as_str();
                if !content.is_empty() {
                    let mut text = text_clone5.lock().unwrap();
                    // Just add the content as-is, we'll clean it up later
                    text.push_str(content);
                }
                Ok(())
            }),
        ],
        ..RewriteStrSettings::default()
    };

    let _ = rewrite_str(&cleaned_html, settings);

    let text = Arc::try_unwrap(text_content).map_or_else(
        |arc| arc.lock().unwrap().clone(),
        |mutex| mutex.into_inner().unwrap(),
    );

    // Clean up the text
    clean_text(&text)
}

/// Clean up extracted text
fn clean_text(text: &str) -> String {
    // First normalize whitespace within lines
    let normalized = text
        .lines()
        .map(|line| {
            // Replace multiple spaces with single space, but preserve the line
            line.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect::<Vec<_>>();

    // Remove excessive empty lines
    let mut result = Vec::new();
    let mut prev_empty = false;

    for line in normalized {
        if line.is_empty() {
            if !prev_empty && !result.is_empty() {
                result.push(String::new());
            }
            prev_empty = true;
        } else {
            result.push(line);
            prev_empty = false;
        }
    }

    // Remove leading/trailing empty lines
    while result.first().is_some_and(String::is_empty) {
        result.remove(0);
    }
    while result.last().is_some_and(String::is_empty) {
        result.pop();
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_html_to_text() {
        let html = r"
            <p>First paragraph.</p>
            <p>Second paragraph.</p>
        ";

        let text = html_to_text(html);
        assert!(text.contains("First paragraph"));
        assert!(text.contains("Second paragraph"));
    }

    #[test]
    fn test_links_preserved_as_text() {
        let html = r#"<p>Check out <a href="https://example.com">this link</a> for more info.</p>"#;
        let text = html_to_text(html);
        assert!(text.contains("Check out this link for more info"));
    }

    #[test]
    fn test_multiple_links_in_content() {
        let html = r#"
            <p>Here's a paragraph with <a href="https://example.com">a link</a> in it.</p>
            <p>And another with <a href="https://test.com">multiple</a> <a href="https://test2.com">links</a>.</p>
        "#;
        let text = html_to_text(html);
        assert!(text.contains("Here's a paragraph with a link in it"));
        assert!(text.contains("And another with multiple links"));
    }

    #[test]
    fn test_lists() {
        let html = r"
            <ul>
                <li>First item</li>
                <li>Second item</li>
            </ul>
        ";

        let text = html_to_text(html);
        assert!(text.contains("• First item"));
        assert!(text.contains("• Second item"));
    }

    #[test]
    fn test_headings() {
        let html = r"
            <h1>Main Title</h1>
            <p>Some content.</p>
            <h2>Subtitle</h2>
            <p>More content.</p>
        ";

        let text = html_to_text(html);
        assert!(text.contains("Main Title"));
        assert!(text.contains("Some content"));
        assert!(text.contains("Subtitle"));
        assert!(text.contains("More content"));
    }

    #[test]
    fn test_skip_scripts_and_styles() {
        let html = r"
            <p>Visible content</p>
            <script>console.log('invisible');</script>
            <style>body { color: red; }</style>
            <p>More visible content</p>
        ";

        let text = html_to_text(html);
        assert!(!text.contains("console.log"));
        assert!(!text.contains("color: red"));
        assert!(text.contains("Visible content"));
        assert!(text.contains("More visible content"));
    }

    #[test]
    fn test_image_alt_text() {
        let html = r#"<p>Here's an image: <img src="test.jpg" alt="Test description"></p>"#;
        let text = html_to_text(html);
        assert!(text.contains("[Image: Test description]"));
    }
}
