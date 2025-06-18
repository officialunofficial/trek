//! Site-specific content extractors

use crate::types::ExtractedContent;
use eyre::Result;
use serde_json::Value;
use tracing::{debug, instrument};

/// Trait for site-specific extractors
pub trait Extractor: Send + Sync {
    /// Check if this extractor can handle the current document
    fn can_extract(&self, url: &str, schema_org_data: &[Value]) -> bool;

    /// Extract content from HTML string
    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent>;

    /// Get the name of this extractor
    fn name(&self) -> &'static str;
}

/// Registry for site-specific extractors
pub struct ExtractorRegistry {
    extractors: Vec<Box<dyn Extractor>>,
}

impl std::fmt::Debug for ExtractorRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExtractorRegistry")
            .field("extractors_count", &self.extractors.len())
            .finish()
    }
}

impl Default for ExtractorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ExtractorRegistry {
    /// Create a new extractor registry
    pub fn new() -> Self {
        // Register built-in extractors
        // TODO: Add extractors as we implement them

        Self {
            extractors: Vec::new(),
        }
    }

    /// Register a new extractor
    pub fn register(&mut self, extractor: Box<dyn Extractor>) {
        debug!("Registering extractor: {}", extractor.name());
        self.extractors.push(extractor);
    }

    /// Find an extractor that can handle the current document
    #[instrument(skip(self, schema_org_data))]
    pub fn find_extractor_from_data(
        &self,
        url: &str,
        schema_org_data: &[Value],
    ) -> Option<&dyn Extractor> {
        for extractor in &self.extractors {
            if extractor.can_extract(url, schema_org_data) {
                debug!("Found matching extractor: {}", extractor.name());
                return Some(extractor.as_ref());
            }
        }
        None
    }
}

/// Generic content extractor (fallback)
pub struct GenericExtractor;

impl Extractor for GenericExtractor {
    fn can_extract(&self, _url: &str, _schema_org_data: &[Value]) -> bool {
        // Generic extractor should not be used as a site-specific extractor
        // It's better to fall back to the main extraction logic
        false
    }

    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent> {
        // Basic extraction logic
        let mut content = ExtractedContent::default();

        // Extract title from HTML
        if let Some(title_start) = html.find("<title>") {
            if let Some(title_end) = html[title_start..].find("</title>") {
                let title = &html[title_start + 7..title_start + title_end];
                content.title = Some(title.trim().to_string());
            }
        }

        Ok(content)
    }

    fn name(&self) -> &'static str {
        "generic"
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // OK to use unwrap/expect in tests
mod tests {
    use super::*;

    #[test]
    fn test_generic_extractor() {
        let extractor = GenericExtractor;
        let html = r"<html><head><title>Test Title</title></head></html>";

        let result = extractor.extract_from_html(html).unwrap();
        assert_eq!(result.title, Some("Test Title".to_string()));
    }

    struct TestExtractor;

    impl Extractor for TestExtractor {
        fn can_extract(&self, url: &str, _schema_org_data: &[Value]) -> bool {
            url.contains("test.com")
        }
        fn extract_from_html(&self, _html: &str) -> Result<ExtractedContent> {
            Ok(ExtractedContent::default())
        }
        fn name(&self) -> &'static str {
            "test"
        }
    }

    #[test]
    fn test_registry() {
        let mut registry = ExtractorRegistry::new();
        registry.register(Box::new(GenericExtractor));

        // GenericExtractor should not match any URL (it returns false for can_extract)
        let extractor = registry.find_extractor_from_data("https://example.com", &[]);
        assert!(extractor.is_none());

        // Test that registry can find extractors when they match
        registry.register(Box::new(TestExtractor));
        let extractor = registry.find_extractor_from_data("https://test.com", &[]);
        assert!(extractor.is_some());
        assert_eq!(extractor.unwrap().name(), "test");
    }
}
