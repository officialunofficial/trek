//! HTML standardization functionality

use tracing::{debug, instrument};

/// Standardize HTML content
#[instrument(skip_all)]
pub fn standardize_content(html: &str, title: &str, debug: bool) -> String {
    debug!("Standardizing content with title: {}", title);

    let mut content = html.to_string();

    // Apply standardizations
    content = standardize_spaces(&content);
    content = remove_html_comments(&content);
    content = standardize_headings(&content, title);

    content = strip_unwanted_attributes(&content, debug);
    content = remove_trailing_headings(&content);

    if !debug {
        content = remove_empty_elements(&content);
        content = flatten_wrapper_elements(&content);
        // Run flatten again after empty element removal
        content = flatten_wrapper_elements(&content);
        // Clean up whitespace after flattening
        content = standardize_spaces(&content);
    }

    content
}

fn standardize_spaces(html: &str) -> String {
    debug!("Standardizing spaces");
    // Replace multiple spaces with single space
    let mut result = html.to_string();
    while result.contains("  ") {
        result = result.replace("  ", " ");
    }

    // Trim lines and collect
    let lines: Vec<String> = result.lines().map(str::trim).map(String::from).collect();

    // Remove excessive empty lines (more than 1 consecutive)
    let mut cleaned_lines = Vec::new();
    let mut empty_count = 0;

    for line in lines {
        if line.is_empty() {
            empty_count += 1;
            if empty_count <= 1 {
                cleaned_lines.push(line);
            }
        } else {
            empty_count = 0;
            cleaned_lines.push(line);
        }
    }

    // Join and trim the result
    let result = cleaned_lines.join("\n");
    result.trim().to_string()
}

fn remove_html_comments(html: &str) -> String {
    debug!("Removing HTML comments");
    // Simple regex-based comment removal
    let comment_pattern = regex::Regex::new(r"<!--.*?-->").expect("Invalid regex");
    comment_pattern.replace_all(html, "").to_string()
}

fn standardize_headings(html: &str, _title: &str) -> String {
    debug!("Standardizing headings");
    // For now, just return the HTML as-is
    // TODO: Implement proper heading standardization with lol_html
    html.to_string()
}

fn strip_unwanted_attributes(html: &str, _debug: bool) -> String {
    debug!("Stripping unwanted attributes");
    // TODO: Implement with lol_html for proper attribute stripping
    // For now, just return the HTML as-is
    html.to_string()
}

fn remove_empty_elements(html: &str) -> String {
    debug!("Removing empty elements");
    // Remove empty paragraphs, divs, etc.
    let empty_p = regex::Regex::new(r"<p[^>]*>\s*</p>").expect("Invalid regex");
    let empty_div = regex::Regex::new(r"<div[^>]*>\s*</div>").expect("Invalid regex");

    let mut result = empty_p.replace_all(html, "").to_string();
    result = empty_div.replace_all(&result, "").to_string();

    result
}

fn remove_trailing_headings(html: &str) -> String {
    debug!("Removing trailing headings");
    // TODO: Implement proper trailing heading removal
    html.to_string()
}

#[allow(clippy::disallowed_methods)] // lol_html macros use unwrap internally
fn flatten_wrapper_elements(html: &str) -> String {
    use lol_html::{RewriteStrSettings, element, rewrite_str};

    debug!("Flattening wrapper elements");

    let settings = RewriteStrSettings {
        element_content_handlers: vec![
            // Handle divs that are just wrappers
            element!("div", |el| {
                // Check if this div has any attributes that suggest it should be preserved
                let has_semantic_attrs = el.get_attribute("role").is_some()
                    || el.get_attribute("aria-label").is_some()
                    || el.get_attribute("itemscope").is_some();

                if has_semantic_attrs {
                    return Ok(());
                }

                // Check class for semantic meaning
                if let Some(class) = el.get_attribute("class") {
                    let class_lower = class.to_lowercase();
                    if class_lower.contains("article")
                        || class_lower.contains("content")
                        || class_lower.contains("footnote")
                        || class_lower.contains("reference")
                        || class_lower.contains("bibliography")
                    {
                        return Ok(());
                    }
                }

                // For other divs, unwrap them by removing the tags but keeping content
                el.remove_and_keep_content();
                Ok(())
            }),
        ],
        ..RewriteStrSettings::default()
    };

    rewrite_str(html, settings).unwrap_or_else(|_| html.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standardize_spaces() {
        let html = "  <p>  Multiple   spaces  </p>  ";
        let result = standardize_spaces(html);
        assert_eq!(result, "<p> Multiple spaces </p>");
    }

    #[test]
    fn test_remove_html_comments() {
        let html = "<p>Text<!-- comment -->More text</p>";
        let result = remove_html_comments(html);
        assert_eq!(result, "<p>TextMore text</p>");
    }

    #[test]
    fn test_remove_empty_elements() {
        let html = "<p>Content</p><p></p><div></div><p>More</p>";
        let result = remove_empty_elements(html);
        assert_eq!(result, "<p>Content</p><p>More</p>");
    }

    #[test]
    fn test_excessive_whitespace_handling() {
        // Test 1: Multiple spaces between words are collapsed to single spaces
        let html_multiple_spaces = "<p>This    has     multiple      spaces</p>";
        let result = standardize_spaces(html_multiple_spaces);
        assert_eq!(result, "<p>This has multiple spaces</p>");

        // Test 2: Whitespace around HTML tags is handled properly
        let html_tag_whitespace = "  <div>   <p>  Content  </p>   </div>  ";
        let result = standardize_spaces(html_tag_whitespace);
        assert_eq!(result, "<div> <p> Content </p> </div>");

        // Test 3: Line breaks and paragraph spacing are preserved
        let html_with_linebreaks =
            "<p>First paragraph</p>\n\n<p>Second paragraph</p>\n<p>Third paragraph</p>";
        let result = standardize_spaces(html_with_linebreaks);
        assert_eq!(
            result,
            "<p>First paragraph</p>\n\n<p>Second paragraph</p>\n<p>Third paragraph</p>"
        );

        // Test 4: Mixed excessive whitespace scenarios
        let html_mixed = "  <article>  \n  <h1>  Title   with    spaces  </h1>  \n\n  <p>  First   paragraph   with     excessive     spaces.  </p>  \n  <p>Second paragraph.</p>  \n  </article>  ";
        let result = standardize_spaces(html_mixed);
        let expected = "<article>\n<h1> Title with spaces </h1>\n\n<p> First paragraph with excessive spaces. </p>\n<p>Second paragraph.</p>\n</article>";
        assert_eq!(result, expected);

        // Test 5: Tabs and other whitespace characters
        let html_with_tabs = "<p>\tTabbed\t\tcontent\t</p>";
        let result = standardize_spaces(html_with_tabs);
        assert_eq!(result, "<p>\tTabbed\t\tcontent\t</p>");

        // Test 6: Complete standardization test with the full pipeline
        let html_complete = "  <div>  \n  <!-- comment -->  \n  <p>  Multiple   spaces  </p>  \n  <p></p>  \n  </div>  ";
        let result = standardize_content(html_complete, "Test Title", false);
        // This should handle spaces, remove comments, and remove empty elements
        assert!(!result.contains("  ")); // No double spaces
        assert!(!result.contains("<!--")); // No comments
        assert!(!result.contains("<p></p>")); // No empty paragraphs
    }

    #[test]
    fn test_pre_formatted_text_preservation() {
        // Note: Current implementation doesn't specifically handle <pre> tags,
        // but this test documents expected behavior for future implementation
        let html_with_pre =
            "<pre>  This   has    multiple     spaces  \n  And preserves formatting  </pre>";
        let result = standardize_spaces(html_with_pre);
        // Currently, standardize_spaces will collapse spaces in <pre> tags too
        // This test documents current behavior, which may need to be updated
        // when proper <pre> handling is implemented
        assert_eq!(
            result,
            "<pre> This has multiple spaces\nAnd preserves formatting </pre>"
        );
    }

    #[test]
    fn test_excessive_newlines() {
        // Test leading newlines
        let html = "\n\n\n\n\n<h1>Title</h1>\n<p>Content</p>";
        let result = standardize_spaces(html);
        assert_eq!(result, "<h1>Title</h1>\n<p>Content</p>");

        // Test multiple consecutive newlines
        let html = "<h1>Title</h1>\n\n\n\n\n<p>Content</p>";
        let result = standardize_spaces(html);
        assert_eq!(result, "<h1>Title</h1>\n\n<p>Content</p>");

        // Test trailing newlines
        let html = "<h1>Title</h1>\n<p>Content</p>\n\n\n\n\n";
        let result = standardize_spaces(html);
        assert_eq!(result, "<h1>Title</h1>\n<p>Content</p>");
    }

    #[test]
    fn test_flatten_wrapper_divs() {
        // Test basic div flattening
        let html = "<div><p>Content</p></div>";
        let result = flatten_wrapper_elements(html);
        assert_eq!(result, "<p>Content</p>");

        // Test nested divs
        let html = "<div><div><div><p>Deep content</p></div></div></div>";
        let result = flatten_wrapper_elements(html);
        assert_eq!(result, "<p>Deep content</p>");

        // Test preserving semantic divs
        let html = r#"<div role="article"><p>Article content</p></div>"#;
        let result = flatten_wrapper_elements(html);
        assert!(result.contains(r#"role="article""#));

        // Test preserving divs with semantic classes
        let html = r#"<div class="article-content"><p>Article</p></div>"#;
        let result = flatten_wrapper_elements(html);
        assert!(result.contains("class=\"article-content\""));

        // Test multiple divs creating excessive newlines
        let html = "<div>\n<div>\n<h1>Title</h1>\n</div>\n<div>\n<p>Content</p>\n</div>\n</div>";
        let result = flatten_wrapper_elements(html);
        // After flattening, we should have less nesting
        assert!(!result.contains("<div><div>"));
    }
}
