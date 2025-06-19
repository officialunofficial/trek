//! Metadata extraction functionality

use crate::CollectedData;
use crate::types::{MetaTagItem, TrekMetadata};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use tracing::instrument;

static TITLE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<title[^>]*>(.*?)</title>").expect("Invalid regex"));

/// Extract metadata from HTML document
#[derive(Debug)]
pub struct MetadataExtractor;

impl MetadataExtractor {
    /// Extract metadata from collected data
    #[instrument(skip(data, url))]
    pub fn extract_from_collected_data(data: &CollectedData, url: Option<&str>) -> TrekMetadata {
        let mut metadata = TrekMetadata::default();

        // Try to extract from schema.org and meta tags first (they have priority)
        if let Some(title) = Self::extract_title_from_data(&data.schema_org_data, &data.meta_tags) {
            metadata.title = title;
        } else if let Some(title) = &data.title {
            // Use HTML title as fallback
            metadata.title.clone_from(title);
        }

        if let Some(description) = Self::extract_description(&data.schema_org_data, &data.meta_tags)
        {
            metadata.description = description;
        }

        if let Some(author) = Self::extract_author(&data.schema_org_data, &data.meta_tags) {
            metadata.author = author;
        }

        if let Some(published) =
            Self::extract_published_date(&data.schema_org_data, &data.meta_tags)
        {
            metadata.published = published;
        }

        if let Some(site) = Self::extract_site_name(&data.meta_tags) {
            metadata.site = site;
        }

        if let Some(image) = Self::extract_image(&data.schema_org_data, &data.meta_tags) {
            metadata.image = image;
        }

        if let Some(favicon) = &data.favicon {
            metadata.favicon.clone_from(favicon);
        }

        // Extract domain from URL if provided
        if let Some(url_str) = url {
            if let Ok(parsed_url) = url::Url::parse(url_str) {
                if let Some(domain) = parsed_url.domain() {
                    // Remove www. prefix if present
                    metadata.domain = domain.trim_start_matches("www.").to_string();
                }
            }
        }

        // Include schema.org data
        metadata.schema_org_data.clone_from(&data.schema_org_data);

        metadata
    }

    /// Extract metadata from HTML string
    #[instrument(skip(html))]
    pub fn extract(html: &str) -> TrekMetadata {
        let mut metadata = TrekMetadata::default();

        // Extract title from HTML
        if let Some(captures) = TITLE_PATTERN.captures(html) {
            if let Some(title_match) = captures.get(1) {
                metadata.title = title_match.as_str().trim().to_string();
            }
        }

        metadata
    }

    fn extract_title_from_data(
        schema_org_data: &[Value],
        meta_tags: &[MetaTagItem],
    ) -> Option<String> {
        // Try schema.org first (prioritize structured data)
        for item in schema_org_data {
            if let Some(headline) = item.get("headline").and_then(Value::as_str) {
                return Some(headline.to_string());
            }
            if let Some(name) = item.get("name").and_then(Value::as_str) {
                return Some(name.to_string());
            }
        }

        // Try Open Graph as fallback
        for tag in meta_tags {
            if tag.property.as_deref() == Some("og:title") {
                return Some(tag.content.clone());
            }
        }

        // Try Twitter Card
        for tag in meta_tags {
            if tag.property.as_deref() == Some("twitter:title") {
                return Some(tag.content.clone());
            }
        }

        None
    }

    fn extract_description(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> Option<String> {
        // Try schema.org first
        for item in schema_org_data {
            if let Some(description) = item.get("description").and_then(Value::as_str) {
                return Some(description.to_string());
            }
        }

        // Try meta description
        for tag in meta_tags {
            if tag.name.as_deref() == Some("description") {
                return Some(tag.content.clone());
            }
        }

        // Try Open Graph
        for tag in meta_tags {
            if tag.property.as_deref() == Some("og:description") {
                return Some(tag.content.clone());
            }
        }

        // Try Twitter Card
        for tag in meta_tags {
            if tag.property.as_deref() == Some("twitter:description") {
                return Some(tag.content.clone());
            }
        }

        None
    }

    fn extract_author(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> Option<String> {
        // Try schema.org
        for item in schema_org_data {
            if let Some(author) = item.get("author") {
                if let Some(name) = author.get("name").and_then(Value::as_str) {
                    return Some(name.to_string());
                }
                if let Some(name) = author.as_str() {
                    return Some(name.to_string());
                }
            }
        }

        // Try byl meta tag (NY Times specific)
        for tag in meta_tags {
            if tag.name.as_deref() == Some("byl") {
                return Some(tag.content.clone());
            }
        }

        // Try author meta tag
        for tag in meta_tags {
            if tag.name.as_deref() == Some("author") {
                return Some(tag.content.clone());
            }
        }

        // Try article:author (but skip if it's a URL)
        for tag in meta_tags {
            if tag.property.as_deref() == Some("article:author") {
                // If it looks like a URL, skip it - we want the actual name
                if !tag.content.starts_with("http://") && !tag.content.starts_with("https://") {
                    return Some(tag.content.clone());
                }
            }
        }

        None
    }

    fn extract_published_date(
        schema_org_data: &[Value],
        meta_tags: &[MetaTagItem],
    ) -> Option<String> {
        // Try schema.org
        for item in schema_org_data {
            if let Some(date) = item.get("datePublished").and_then(Value::as_str) {
                return Some(date.to_string());
            }
        }

        // Try meta tags
        for tag in meta_tags {
            if tag.property.as_deref() == Some("article:published_time") {
                return Some(tag.content.clone());
            }
            if tag.name.as_deref() == Some("publish_date") {
                return Some(tag.content.clone());
            }
        }

        None
    }

    fn extract_site_name(meta_tags: &[MetaTagItem]) -> Option<String> {
        // Try Open Graph
        for tag in meta_tags {
            if tag.property.as_deref() == Some("og:site_name") {
                return Some(tag.content.clone());
            }
        }

        // Try Twitter
        for tag in meta_tags {
            if tag.name.as_deref() == Some("twitter:site") {
                return Some(tag.content.clone());
            }
        }

        None
    }

    fn extract_image(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> Option<String> {
        // Try schema.org first
        for item in schema_org_data {
            if let Some(image) = item.get("image") {
                // Handle both string and object representations
                if let Some(url) = image.as_str() {
                    return Some(url.to_string());
                }
                // Handle ImageObject
                if let Some(url) = image.get("url").and_then(Value::as_str) {
                    return Some(url.to_string());
                }
                // Handle array of images
                if let Some(images) = image.as_array() {
                    if let Some(first_image) = images.first() {
                        if let Some(url) = first_image.as_str() {
                            return Some(url.to_string());
                        }
                        if let Some(url) = first_image.get("url").and_then(Value::as_str) {
                            return Some(url.to_string());
                        }
                    }
                }
            }
        }

        // Try Open Graph
        for tag in meta_tags {
            if tag.property.as_deref() == Some("og:image") {
                return Some(tag.content.clone());
            }
        }

        // Try Twitter
        for tag in meta_tags {
            if tag.name.as_deref() == Some("twitter:image") {
                return Some(tag.content.clone());
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title_from_html() {
        let html = r"<html><head><title>Test Title</title></head></html>";
        let metadata = MetadataExtractor::extract(html);
        assert_eq!(metadata.title, "Test Title");
    }

    #[test]
    fn test_extract_from_collected_data() {
        let mut data = CollectedData::default();
        data.meta_tags.push(MetaTagItem {
            name: Some("description".to_string()),
            property: None,
            content: "Test description".to_string(),
        });

        let metadata = MetadataExtractor::extract_from_collected_data(&data, None);
        assert_eq!(metadata.description, "Test description");
    }
}
