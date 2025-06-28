//! Trek - A modern web content extraction library
//!
//! Trek removes clutter from web pages and extracts clean, readable content.
//! It's designed as a modern alternative to Mozilla Readability with enhanced
//! features like mobile-aware extraction and consistent HTML standardization.

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::multiple_crate_versions,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc
)]

use eyre::Result;
use lol_html::{RewriteStrSettings, element, rewrite_str, text};
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tracing::{debug, info, instrument};

pub mod constants;
pub mod elements;
pub mod error;
pub mod extractor;
pub mod extractors;
pub mod html_to_text;
pub mod metadata;
pub mod scoring;
pub mod standardize;
pub mod types;
pub mod utils;

#[cfg(target_arch = "wasm32")]
pub mod wasm;

use crate::extractor::{ExtractorRegistry, GenericExtractor};
use crate::extractors::FarcasterExtractor;
use crate::metadata::MetadataExtractor;
pub use crate::types::{MetaTagItem, TrekOptions, TrekResponse};

/// Main Trek struct for content extraction
#[derive(Debug)]
pub struct Trek {
    options: TrekOptions,
    extractor_registry: ExtractorRegistry,
}

impl Trek {
    /// Create a new Trek instance with the given options
    #[instrument(skip(options))]
    pub fn new(options: TrekOptions) -> Self {
        let mut extractor_registry = ExtractorRegistry::new();
        // Register built-in extractors
        extractor_registry.register(Box::new(GenericExtractor));
        extractor_registry.register(Box::new(FarcasterExtractor));

        Self {
            options,
            extractor_registry,
        }
    }

    /// Parse HTML content and extract the main content
    #[instrument(skip(self, html))]
    pub fn parse(&self, html: &str) -> Result<TrekResponse> {
        let start_time = utils::current_time_ms();

        // First pass: collect metadata and schema.org data
        let collected_data = self.collect_initial_data(html)?;

        // Extract metadata
        let metadata = MetadataExtractor::extract_from_collected_data(
            &collected_data,
            self.options.url.as_deref(),
        );

        // Try site-specific extractor first
        let url = self.options.url.as_deref().unwrap_or("");
        if let Some(extractor) = self
            .extractor_registry
            .find_extractor_from_data(url, &collected_data.schema_org_data)
        {
            info!("Using site-specific extractor: {}", extractor.name());
            let extracted = extractor.extract_from_html(html)?;

            #[allow(clippy::redundant_clone)]
            let mut final_metadata = metadata.clone();
            if let Some(title) = extracted.title {
                final_metadata.title = title;
            }
            if let Some(author) = extracted.author {
                final_metadata.author = author;
            }
            if let Some(published) = extracted.published {
                final_metadata.published = published;
            }

            let content = extracted.content_html.unwrap_or_default();
            final_metadata.word_count = utils::count_words(&content);
            final_metadata.parse_time = utils::current_time_ms() - start_time;

            return Ok(TrekResponse {
                content,
                content_markdown: None,
                extractor_type: Some(extractor.name().to_string()),
                meta_tags: collected_data.meta_tags.clone(),
                metadata: final_metadata,
            });
        }

        // Fall back to generic extraction
        let result = self.parse_internal(html, &metadata, &collected_data.meta_tags, start_time)?;

        // If result has very little content, try again without clutter removal
        if result.metadata.word_count < 200
            && (self.options.removal.remove_exact_selectors
                || self.options.removal.remove_partial_selectors)
        {
            info!(
                "Initial parse returned very little content, trying again without clutter removal"
            );
            let mut retry_options = self.options.clone();
            retry_options.removal.remove_exact_selectors = false;
            retry_options.removal.remove_partial_selectors = false;

            let retry_trek = Self::new(retry_options);
            let retry_metadata = MetadataExtractor::extract_from_collected_data(
                &collected_data,
                self.options.url.as_deref(),
            );
            if let Ok(retry_result) = retry_trek.parse_internal(
                html,
                &retry_metadata,
                &collected_data.meta_tags,
                start_time,
            ) {
                if retry_result.metadata.word_count > result.metadata.word_count {
                    debug!("Retry produced more content");
                    return Ok(retry_result);
                }
            }
        }

        Ok(result)
    }

    fn parse_internal(
        &self,
        html: &str,
        metadata: &types::TrekMetadata,
        meta_tags: &[MetaTagItem],
        start_time: u64,
    ) -> Result<TrekResponse> {
        // Find and extract main content
        let main_content = self.extract_main_content(html);

        // Extract just the body content first
        let body_content = self.extract_body_content(&main_content);

        // Remove clutter if enabled
        let cleaned_content = if self.options.removal.remove_exact_selectors
            || self.options.removal.remove_partial_selectors
        {
            let result = self.remove_clutter(&body_content)?;
            if self.options.debug {
                debug!("After clutter removal, content length: {}", result.len());
            }
            result
        } else {
            body_content
        };

        // Standardize content
        let final_content =
            standardize::standardize_content(&cleaned_content, &metadata.title, self.options.debug);

        let mut final_metadata = metadata.clone();
        final_metadata.word_count = utils::count_words(&final_content);
        final_metadata.parse_time = utils::current_time_ms() - start_time;

        // If no metadata image found, try to extract first suitable image from content
        if final_metadata.image.is_empty() {
            if let Some(first_image) = Self::extract_first_image_from_content(&final_content) {
                debug!("Found first image in content: {}", first_image);
                final_metadata.image = first_image;
            }
        }

        Ok(TrekResponse {
            content: final_content,
            content_markdown: None,
            extractor_type: None,
            meta_tags: meta_tags.to_vec(),
            metadata: final_metadata,
        })
    }

    #[allow(clippy::disallowed_methods, clippy::unused_self)] // lol_html macros use unwrap internally
    fn collect_initial_data(&self, html: &str) -> Result<CollectedData> {
        let collected_data = Arc::new(Mutex::new(CollectedData::default()));
        let data_clone = Arc::clone(&collected_data);
        let data_clone2 = Arc::clone(&collected_data);

        // For script content, we need to track state
        let script_content = Arc::new(Mutex::new(String::new()));
        let script_clone = Arc::clone(&script_content);

        // For title content, we need to track state
        let title_content = Arc::new(Mutex::new(String::new()));
        let title_clone = Arc::clone(&title_content);
        let data_clone3 = Arc::clone(&collected_data);

        let data_clone4 = Arc::clone(&collected_data);

        let settings = RewriteStrSettings {
            element_content_handlers: vec![
                // Collect meta tags
                element!("meta[name], meta[property]", move |el| {
                    if let Some(content) = el.get_attribute("content") {
                        let mut data = data_clone.lock().expect("Failed to acquire lock");

                        // Check for fc:frame meta tag
                        if el.get_attribute("name").as_deref() == Some("fc:frame") {
                            data.mini_app_embed = Some(content.clone());
                        }

                        data.meta_tags.push(MetaTagItem {
                            name: el.get_attribute("name"),
                            property: el.get_attribute("property"),
                            content: utils::decode_html_entities(&content),
                        });
                    }
                    Ok(())
                }),
                // Collect favicon
                element!("link[rel~=icon], link[rel~=shortcut]", move |el| {
                    if let Some(href) = el.get_attribute("href") {
                        let mut data = data_clone4.lock().expect("Failed to acquire lock");
                        // Prefer icon over shortcut icon
                        if data.favicon.is_none()
                            || el.get_attribute("rel").as_deref() == Some("icon")
                        {
                            data.favicon = Some(href);
                        }
                    }
                    Ok(())
                }),
                // Collect title tag
                element!("title", move |_el| {
                    // Clear the content buffer for this title
                    {
                        let mut content = title_clone.lock().expect("Failed to acquire lock");
                        content.clear();
                    }
                    Ok(())
                }),
                // Collect text within title tag
                text!("title", move |t| {
                    {
                        let mut content = title_content.lock().expect("Failed to acquire lock");
                        content.push_str(t.as_str());

                        // Check if this is the last chunk
                        if t.last_in_text_node() {
                            let title_str = content.trim().to_string();
                            drop(content); // Explicitly drop before acquiring next lock
                            let mut data = data_clone3.lock().expect("Failed to acquire lock");
                            data.title = Some(title_str);
                        }
                    }
                    Ok(())
                }),
                // Collect schema.org data
                element!(r#"script[type="application/ld+json"]"#, move |_el| {
                    // Clear the content buffer for this script
                    {
                        let mut content = script_clone.lock().expect("Failed to acquire lock");
                        content.clear();
                    }
                    Ok(())
                }),
                // Collect text within script tags
                text!(r#"script[type="application/ld+json"]"#, move |t| {
                    {
                        let mut content = script_content.lock().expect("Failed to acquire lock");
                        content.push_str(t.as_str());

                        // Check if this is the last chunk
                        if t.last_in_text_node() {
                            // Parse the complete JSON
                            if let Ok(json_data) = serde_json::from_str::<Value>(&content) {
                                drop(content); // Drop before acquiring next lock
                                let mut data = data_clone2.lock().expect("Failed to acquire lock");
                                if let Some(graph) =
                                    json_data.get("@graph").and_then(Value::as_array)
                                {
                                    data.schema_org_data.extend(graph.clone());
                                } else {
                                    data.schema_org_data.push(json_data);
                                }
                            }
                        }
                    }
                    Ok(())
                }),
            ],
            ..RewriteStrSettings::default()
        };

        rewrite_str(html, settings)?;

        let data = Arc::try_unwrap(collected_data).map_or_else(
            |arc| arc.lock().expect("Failed to acquire lock").clone(),
            |mutex| mutex.into_inner().expect("Failed to get inner value"),
        );

        Ok(data)
    }

    #[allow(clippy::unused_self, clippy::disallowed_methods)] // lol_html macros use unwrap internally
    fn extract_main_content(&self, html: &str) -> String {
        // For now, just return the HTML as-is
        // The actual content identification happens through the remove_clutter phase
        html.to_string()
    }

    #[allow(clippy::unused_self)]
    fn extract_body_content(&self, html: &str) -> String {
        // Extract just the content inside the body tag
        if let Some(body_start) = html.find("<body") {
            if let Some(tag_end) = html[body_start..].find('>') {
                let content_start = body_start + tag_end + 1;
                if let Some(body_end) = html.rfind("</body>") {
                    let content = html[content_start..body_end].trim();
                    // Remove leading newlines
                    return content.trim_start_matches('\n').to_string();
                }
            }
        }

        // If no body tags found, return as-is
        html.trim_start_matches('\n').to_string()
    }

    #[allow(clippy::disallowed_methods)] // lol_html macros use unwrap internally
    fn extract_first_image_from_content(html: &str) -> Option<String> {
        use lol_html::{RewriteStrSettings, element, rewrite_str};

        let first_image = Arc::new(Mutex::new(None::<String>));
        let image_clone = Arc::clone(&first_image);

        let settings = RewriteStrSettings {
            element_content_handlers: vec![element!("img", move |el| {
                let mut image_guard = image_clone.lock().expect("Failed to acquire lock");

                // Skip if we already found an image
                if image_guard.is_some() {
                    return Ok(());
                }

                // Get the src attribute
                if let Some(src) = el.get_attribute("src") {
                    // Skip data URLs, tracking pixels, and small images
                    if !src.starts_with("data:") && !src.is_empty() {
                        // Check dimensions if available
                        let width = el
                            .get_attribute("width")
                            .and_then(|w| w.parse::<u32>().ok())
                            .unwrap_or(100);
                        let height = el
                            .get_attribute("height")
                            .and_then(|h| h.parse::<u32>().ok())
                            .unwrap_or(100);

                        // Skip small images (likely icons or tracking pixels)
                        if width >= 50 && height >= 50 {
                            *image_guard = Some(src);
                        }
                    }
                }
                drop(image_guard);

                Ok(())
            })],
            ..RewriteStrSettings::default()
        };

        // Process the HTML
        let _ = rewrite_str(html, settings).ok()?;

        // Extract the result
        match Arc::try_unwrap(first_image) {
            Ok(mutex) => mutex.into_inner().expect("Failed to get inner value"),
            Err(arc) => {
                let guard = arc.lock().expect("Failed to acquire lock");
                guard.clone()
            }
        }
    }

    #[allow(clippy::unused_self, clippy::disallowed_methods)] // lol_html macros use unwrap internally
    fn remove_clutter(&self, html: &str) -> Result<String> {
        use crate::constants::{PARTIAL_SELECTORS, TEST_ATTRIBUTES};
        use lol_html::html_content::ContentType;

        // Capture options in local variables for the closure
        let remove_exact = self.options.removal.remove_exact_selectors;
        let remove_partial = self.options.removal.remove_partial_selectors;

        // Use comments to mark content for removal
        let settings = RewriteStrSettings {
            element_content_handlers: vec![
                // Remove common non-content elements by tag name
                element!(
                    "script, style, nav, footer, header, aside, noscript",
                    move |el| {
                        if remove_exact {
                            el.before("<!--REMOVE-->", ContentType::Html);
                            el.after("<!--/REMOVE-->", ContentType::Html);
                            el.remove();
                        }
                        Ok(())
                    }
                ),
                // Remove elements matching class/id selectors
                element!(
                    "div, section, article, main, span, p, ul, ol, li, h1, h2, h3, h4, h5, h6",
                    move |el| {
                        let mut should_remove = false;

                        if remove_exact {
                            // Check for .navigation, .sidebar, etc.
                            if let Some(class_attr) = el.get_attribute("class") {
                                for class in class_attr.split_whitespace() {
                                    if class == "navigation" || class == "sidebar" {
                                        should_remove = true;
                                        break;
                                    }
                                }
                            }
                        }

                        if !should_remove && remove_partial {
                            // Check each test attribute for partial matches
                            for attr in TEST_ATTRIBUTES {
                                if let Some(value) = el.get_attribute(attr) {
                                    let value_lower = value.to_lowercase();
                                    for pattern in PARTIAL_SELECTORS {
                                        if value_lower.contains(pattern) {
                                            should_remove = true;
                                            break;
                                        }
                                    }
                                }
                                if should_remove {
                                    break;
                                }
                            }
                        }

                        if should_remove {
                            el.before("<!--REMOVE-->", ContentType::Html);
                            el.after("<!--/REMOVE-->", ContentType::Html);
                            el.remove();
                        }

                        Ok(())
                    }
                ),
            ],
            ..RewriteStrSettings::default()
        };

        let result = rewrite_str(html, settings)?;

        // Second pass: Remove content between REMOVE markers (including newlines)
        let remove_pattern = regex::Regex::new(r"(?s)<!--REMOVE-->.*?<!--/REMOVE-->").unwrap();
        let cleaned = remove_pattern.replace_all(&result, "").to_string();

        Ok(cleaned)
    }
}

#[derive(Debug, Clone, Default)]
pub struct CollectedData {
    pub meta_tags: Vec<MetaTagItem>,
    pub schema_org_data: Vec<Value>,
    pub title: Option<String>,
    pub favicon: Option<String>,
    pub mini_app_embed: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let options = TrekOptions::default();
        let _trek = Trek::new(options);
    }

    #[test]
    fn test_fallback_image_extraction() {
        let trek = Trek::new(TrekOptions::default());

        // HTML with no og:image meta tag but images in content
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Test Article</title>
                <meta name="description" content="Test description">
            </head>
            <body>
                <article>
                    <h1>Article Title</h1>
                    <img src="/tracking.gif" width="1" height="1" alt="">
                    <p>Some text here</p>
                    <img src="https://example.com/main-image.jpg" width="800" height="600" alt="Main article image">
                    <p>More content</p>
                    <img src="https://example.com/another-image.jpg" alt="Another image">
                </article>
            </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();

        // Should extract the first suitable image (not the tracking pixel)
        assert_eq!(result.metadata.image, "https://example.com/main-image.jpg");
    }

    #[test]
    fn test_no_fallback_when_og_image_exists() {
        let trek = Trek::new(TrekOptions::default());

        // HTML with og:image meta tag
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Test Article</title>
                <meta property="og:image" content="https://example.com/og-image.jpg">
            </head>
            <body>
                <article>
                    <h1>Article Title</h1>
                    <img src="https://example.com/content-image.jpg" width="800" height="600" alt="Content image">
                </article>
            </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();

        // Should use og:image, not content image
        assert_eq!(result.metadata.image, "https://example.com/og-image.jpg");
    }

    #[test]
    fn test_no_suitable_images() {
        let trek = Trek::new(TrekOptions::default());

        // HTML with only small/tracking images
        let html = r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Test Article</title>
            </head>
            <body>
                <article>
                    <h1>Article Title</h1>
                    <img src="/tracking.gif" width="1" height="1" alt="">
                    <img src="/icon.png" width="16" height="16" alt="Icon">
                    <img src="data:image/gif;base64,R0lGODlhAQABAIAAAAAAAP///yH5BAEAAAAALAAAAAABAAEAAAIBRAA7" alt="">
                    <p>Content without suitable images</p>
                </article>
            </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();

        // Should have empty image since no suitable images found
        assert_eq!(result.metadata.image, "");
    }

    #[test]
    #[allow(clippy::disallowed_methods)] // OK to use unwrap in tests
    fn test_basic_extraction() {
        let trek = Trek::new(TrekOptions::default());
        let html = r#"
            <html>
                <head>
                    <title>Test Page</title>
                    <meta name="description" content="A test page">
                </head>
                <body>
                    <article>
                        <h1>Main Title</h1>
                        <p>This is a test paragraph with some content.</p>
                    </article>
                </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();
        assert!(result.metadata.word_count > 0);
        assert_eq!(result.metadata.title, "Test Page");
        assert_eq!(result.metadata.description, "A test page");
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn test_debug_extraction() {
        let trek = Trek::new(TrekOptions {
            debug: true,
            ..Default::default()
        });

        let html = r#"
            <html>
                <body>
                    <main>
                        <h1>Main Content</h1>
                        <p>First paragraph here.</p>
                        <p>Second paragraph here.</p>
                    </main>
                </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();
        println!("Debug - Content: {}", result.content);
        println!("Debug - Word count: {}", result.metadata.word_count);

        assert!(!result.content.is_empty(), "Should have content");
        assert!(result.metadata.word_count > 0, "Should count words");
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn test_remove_clutter() {
        let trek = Trek::new(TrekOptions::default());

        let html = r#"
            <html>
                <body>
                    <nav>Navigation</nav>
                    <article>Content</article>
                    <footer>Footer</footer>
                </body>
            </html>
        "#;

        let result = trek.remove_clutter(html).unwrap();
        println!("After clutter removal: {result}");

        assert!(!result.contains("<nav>"), "Should remove nav");
        assert!(!result.contains("<footer>"), "Should remove footer");
        assert!(result.contains("<article>"), "Should keep article");
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn test_html_tags_preserved_in_extraction() {
        let trek = Trek::new(TrekOptions::default());

        let html = r#"
            <html>
                <head>
                    <title>Test Article</title>
                </head>
                <body>
                    <article>
                        <h1>Main Title</h1>
                        <p>This article references <a href="https://example.com">an important source</a> for context.</p>
                        <p>You can also check <a href="https://test.com">this link</a> and <a href="https://another.com">another link</a> for more info.</p>
                        <p>This text is <strong>very important</strong> and <em>emphasized</em>.</p>
                    </article>
                </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();
        println!("Extracted content: {:?}", result.content);

        // Should preserve HTML tags
        assert!(
            result.content.contains("<a href="),
            "Should preserve anchor tags"
        );
        assert!(
            result.content.contains("<strong>"),
            "Should preserve strong tags"
        );
        assert!(result.content.contains("<em>"), "Should preserve em tags");
        assert!(
            result.content.contains("</a>"),
            "Should preserve closing anchor tags"
        );
        assert!(
            result.content.contains("</strong>"),
            "Should preserve closing strong tags"
        );
        assert!(
            result.content.contains("</em>"),
            "Should preserve closing em tags"
        );

        // Should preserve content
        assert!(
            result.content.contains("an important source"),
            "Should preserve link text"
        );
        assert!(
            result.content.contains("very important"),
            "Should preserve strong text"
        );
        assert!(
            result.content.contains("emphasized"),
            "Should preserve em text"
        );
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn test_whitespace_handling_in_extraction() {
        let trek = Trek::new(TrekOptions::default());

        let html = r#"
            <html>
                <head>
                    <title>Test Article</title>
                </head>
                <body>
                    <article>
                        <h1>Title   with    excessive     spaces</h1>
                        <p>This    paragraph    has     multiple      spaces     between    words.</p>
                        <p>
                            This paragraph has
                            line breaks and     multiple
                            spaces    throughout.
                        </p>
                        <p>Normal paragraph.</p>
                    </article>
                </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();
        println!("Whitespace test result:\n{}", result.content);

        // Should collapse multiple spaces
        assert!(
            !result.content.contains("   "),
            "Should not have triple spaces"
        );
        assert!(
            !result.content.contains("  "),
            "Should not have double spaces"
        );

        // Should preserve paragraph structure
        assert!(result.content.contains("<p>"), "Should have paragraph tags");
        assert!(
            result.content.contains("</p>"),
            "Should have closing paragraph tags"
        );

        // Content should be readable
        assert!(
            result.content.contains("Title with excessive spaces"),
            "Title should be normalized"
        );
        assert!(
            result
                .content
                .contains("This paragraph has multiple spaces between words"),
            "First paragraph should be normalized"
        );
    }

    #[test]
    #[allow(clippy::disallowed_methods)]
    fn test_div_flattening_reduces_newlines() {
        let trek = Trek::new(TrekOptions::default());

        let html = r#"
            <html>
                <head>
                    <title>Test Article</title>
                </head>
                <body>
                    <div>
                        <div>
                            <div>
                                <h1>How A.I. Sees Us</h1>
                            </div>
                        </div>
                        <div>
                            <div>
                                <p>Not only can A.I. now make these assessments with remarkable accuracy.</p>
                            </div>
                        </div>
                    </div>
                </body>
            </html>
        "#;

        let result = trek.parse(html).unwrap();
        println!("Div flattening result:\n{}", result.content);

        // Should not have excessive newlines at the start
        assert!(
            !result.content.starts_with("\n\n\n"),
            "Should not start with multiple newlines"
        );

        // Should have flattened the divs
        let div_count = result.content.matches("<div").count();
        assert!(
            div_count == 0,
            "All wrapper divs should be flattened, found {div_count} divs"
        );

        // Content should be clean
        assert!(result.content.contains("<h1>How A.I. Sees Us</h1>"));
        assert!(
            result
                .content
                .contains("<p>Not only can A.I. now make these assessments")
        );
    }
}
