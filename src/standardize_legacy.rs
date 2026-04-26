//! HTML standardization functionality

use once_cell::sync::Lazy;
use regex::Regex;
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

    // Rewrite known social/video embed iframes to plain links so downstream
    // markdown / text consumers don't drop them on the floor. Quick-win (c).
    content = rewrite_embed_iframes(&content);

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

// `<iframe ... src="https://www.youtube.com/embed/VIDEO_ID?...">...</iframe>`
static YOUTUBE_IFRAME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<iframe[^>]*\bsrc\s*=\s*["'](?:https?:)?//(?:www\.)?youtube(?:-nocookie)?\.com/embed/([A-Za-z0-9_\-]+)[^"']*["'][^>]*>\s*</iframe>"#,
    )
    .expect("invalid youtube iframe regex")
});

// `<iframe ... src="https://platform.twitter.com/embed/Tweet.html?id=...">` or
// `<iframe ... src="https://twitter.com/{user}/status/{id}">`
static TWITTER_IFRAME_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?is)<iframe[^>]*\bsrc\s*=\s*["'](?:https?:)?//(?:www\.)?(?:twitter|x)\.com/([A-Za-z0-9_]+)/status/(\d+)[^"']*["'][^>]*>\s*</iframe>"#,
    )
    .expect("invalid twitter iframe regex")
});

fn rewrite_embed_iframes(html: &str) -> String {
    debug!("Rewriting known embed iframes to plain links");
    let after_yt = YOUTUBE_IFRAME_RE.replace_all(html, |caps: &regex::Captures| {
        let id = &caps[1];
        format!(r#"<a href="https://www.youtube.com/watch?v={id}">YouTube: {id}</a>"#)
    });
    let after_tw = TWITTER_IFRAME_RE.replace_all(&after_yt, |caps: &regex::Captures| {
        let user = &caps[1];
        let id = &caps[2];
        format!(r#"<a href="https://twitter.com/{user}/status/{id}">Tweet by @{user}: {id}</a>"#,)
    });
    after_tw.into_owned()
}

fn standardize_spaces(html: &str) -> String {
    debug!("Standardizing spaces");
    // We carve out `<pre>...</pre>` regions so internal whitespace is
    // preserved verbatim. Code fences must keep indentation and runs of
    // spaces; the rest of the document still gets normalized.
    // Also carve out block-style `<code class="...block...">` which some
    // sites (lean-verso et al.) use as the standalone code container.
    let preserve_re = regex::Regex::new(
        r#"(?is)<pre[^>]*>.*?</pre>|<code\b[^>]*\bclass\s*=\s*"[^"]*\bblock\b[^"]*"[^>]*>.*?</code>"#,
    )
    .expect("preserve regex");
    let mut pieces: Vec<(bool, String)> = Vec::new();
    let mut cursor = 0usize;
    for m in preserve_re.find_iter(html) {
        if m.start() > cursor {
            pieces.push((false, html[cursor..m.start()].to_string()));
        }
        pieces.push((true, html[m.start()..m.end()].to_string()));
        cursor = m.end();
    }
    if cursor < html.len() {
        pieces.push((false, html[cursor..].to_string()));
    }
    if pieces.is_empty() {
        pieces.push((false, html.to_string()));
    }

    let mut out_parts: Vec<String> = Vec::with_capacity(pieces.len());
    for (is_pre, mut chunk) in pieces {
        if is_pre {
            out_parts.push(chunk);
            continue;
        }
        // Replace multiple spaces with single space.
        while chunk.contains("  ") {
            chunk = chunk.replace("  ", " ");
        }
        // Trim lines and collect.
        let lines: Vec<String> = chunk.lines().map(str::trim).map(String::from).collect();
        let mut cleaned_lines: Vec<String> = Vec::with_capacity(lines.len());
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
        out_parts.push(cleaned_lines.join("\n"));
    }
    out_parts.join("").trim().to_string()
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

    // Carve out `<pre>...</pre>` regions before flattening so we don't
    // unwrap structural divs that highlighters use as line containers.
    let pre_re = regex::Regex::new(r"(?is)<pre[^>]*>.*?</pre>").expect("pre regex");
    let mut pieces: Vec<(bool, String)> = Vec::new();
    let mut cursor = 0usize;
    for m in pre_re.find_iter(html) {
        if m.start() > cursor {
            pieces.push((false, html[cursor..m.start()].to_string()));
        }
        pieces.push((true, html[m.start()..m.end()].to_string()));
        cursor = m.end();
    }
    if cursor < html.len() {
        pieces.push((false, html[cursor..].to_string()));
    }
    if pieces.is_empty() {
        pieces.push((false, html.to_string()));
    }

    fn flatten_chunk(html: &str) -> String {
        let settings = RewriteStrSettings {
            element_content_handlers: vec![element!("div", |el| {
                let has_semantic_attrs = el.get_attribute("role").is_some()
                    || el.get_attribute("aria-label").is_some()
                    || el.get_attribute("aria-hidden").is_some()
                    || el.get_attribute("hidden").is_some()
                    || el.get_attribute("itemscope").is_some()
                    || el.get_attribute("data-callout").is_some()
                    || el.get_attribute("data-callout-fold").is_some()
                    || el.get_attribute("data-callout-metadata").is_some()
                    || el.get_attribute("data-floating-buttons").is_some()
                    || el.get_attribute("data-fade-overlay").is_some()
                    || el.get_attribute("data-rehype-pretty-code-figure").is_some();
                if has_semantic_attrs {
                    return Ok(());
                }
                if let Some(class) = el.get_attribute("class") {
                    let class_lower = class.to_lowercase();
                    if class_lower.contains("article")
                        || class_lower.contains("content")
                        || class_lower.contains("footnote")
                        || class_lower.contains("reference")
                        || class_lower.contains("bibliography")
                        || class_lower.contains("callout")
                    {
                        return Ok(());
                    }
                    // Preserve code-block highlight wrappers — they carry the
                    // language hint that the standardize pass uses to label
                    // the canonical fenced code block.
                    if class_lower.split_whitespace().any(|t| {
                        t.starts_with("language-")
                            || t.starts_with("lang-")
                            || t == "highlight"
                            || t == "highlighter-rouge"
                            || t == "expressive-code"
                            || t == "code-block"
                            || t == "highlight-source"
                            || t.starts_with("highlight-source-")
                    }) {
                        return Ok(());
                    }
                }
                el.remove_and_keep_content();
                Ok(())
            })],
            ..RewriteStrSettings::default()
        };
        rewrite_str(html, settings).unwrap_or_else(|_| html.to_string())
    }

    let mut out = String::with_capacity(html.len());
    for (is_pre, chunk) in pieces {
        if is_pre {
            out.push_str(&chunk);
        } else {
            out.push_str(&flatten_chunk(&chunk));
        }
    }
    out
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
        // standardize_spaces preserves whitespace inside `<pre>` so code
        // blocks emerge from the pipeline with their indentation intact.
        let html_with_pre =
            "<pre>  This   has    multiple     spaces  \n  And preserves formatting  </pre>";
        let result = standardize_spaces(html_with_pre);
        assert_eq!(
            result,
            "<pre>  This   has    multiple     spaces  \n  And preserves formatting  </pre>"
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
    fn test_rewrite_youtube_iframe() {
        let html = r#"<p>before</p><iframe width="560" height="315" src="https://www.youtube.com/embed/dQw4w9WgXcQ" frameborder="0"></iframe><p>after</p>"#;
        let out = rewrite_embed_iframes(html);
        assert!(out.contains(
            r#"<a href="https://www.youtube.com/watch?v=dQw4w9WgXcQ">YouTube: dQw4w9WgXcQ</a>"#
        ));
        assert!(!out.contains("<iframe"));
    }

    #[test]
    fn test_rewrite_twitter_iframe() {
        let html = r#"<iframe src="https://twitter.com/jack/status/20"></iframe>"#;
        let out = rewrite_embed_iframes(html);
        assert!(
            out.contains(r#"<a href="https://twitter.com/jack/status/20">Tweet by @jack: 20</a>"#)
        );
    }

    #[test]
    fn test_rewrite_x_status_iframe() {
        let html = r#"<iframe src="https://x.com/jack/status/20"></iframe>"#;
        let out = rewrite_embed_iframes(html);
        assert!(
            out.contains(r#"<a href="https://twitter.com/jack/status/20">Tweet by @jack: 20</a>"#)
        );
    }

    #[test]
    fn test_rewrite_does_not_touch_unknown_iframe() {
        let html = r#"<iframe src="https://example.com/foo"></iframe>"#;
        let out = rewrite_embed_iframes(html);
        assert_eq!(out, html);
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
