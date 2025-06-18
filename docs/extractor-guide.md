# Extractor Development Guide

This guide explains how to create custom extractors for Trek to handle site-specific content extraction.

## Understanding Extractors

Extractors are components that implement site-specific logic for content extraction. They allow Trek to provide optimized extraction for different websites while maintaining a consistent API.

## The Extractor Trait

All extractors must implement the `Extractor` trait:

```rust
pub trait Extractor: Send + Sync {
    /// Check if this extractor can handle the given URL and schema data
    fn can_extract(&self, url: &str, schema_org_data: &[Value]) -> bool;
    
    /// Extract content from HTML
    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent>;
    
    /// Return the name of this extractor
    fn name(&self) -> &'static str;
}
```

### ExtractedContent Structure

```rust
pub struct ExtractedContent {
    pub title: String,
    pub content: String,
    pub author: Option<String>,
    pub published_at: Option<String>,
    pub excerpt: Option<String>,
    pub site_name: Option<String>,
    pub content_type: Option<String>,
}
```

## Creating a Custom Extractor

### Step 1: Create the Extractor File

Create a new file in `src/extractors/` directory. For example, `src/extractors/medium.rs`:

```rust
use crate::extractor::{Extractor, ExtractedContent};
use crate::error::Result;
use scraper::{Html, Selector};
use serde_json::Value;

pub struct MediumExtractor;

impl Extractor for MediumExtractor {
    fn can_extract(&self, url: &str, _schema_org_data: &[Value]) -> bool {
        url.contains("medium.com") || url.contains("towardsdatascience.com")
    }
    
    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent> {
        let document = Html::parse_document(html);
        
        // Extract title
        let title = extract_title(&document)?;
        
        // Extract main content
        let content = extract_content(&document)?;
        
        // Extract metadata
        let author = extract_author(&document);
        let published_at = extract_publish_date(&document);
        
        Ok(ExtractedContent {
            title,
            content,
            author,
            published_at,
            excerpt: None,
            site_name: Some("Medium".to_string()),
            content_type: Some("BlogPosting".to_string()),
        })
    }
    
    fn name(&self) -> &'static str {
        "MediumExtractor"
    }
}
```

### Step 2: Implement Extraction Logic

```rust
fn extract_title(document: &Html) -> Result<String> {
    // Try multiple selectors in priority order
    let selectors = [
        "h1[data-testid='storyTitle']",
        "h1.pw-post-title",
        "h1",
    ];
    
    for selector_str in &selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                let title = element.text().collect::<String>().trim().to_string();
                if !title.is_empty() {
                    return Ok(title);
                }
            }
        }
    }
    
    Err("Could not extract title".into())
}

fn extract_content(document: &Html) -> Result<String> {
    let article_selector = Selector::parse("article").unwrap();
    
    if let Some(article) = document.select(&article_selector).next() {
        // Clean up the content
        let mut content = article.html();
        
        // Remove unwanted elements
        content = remove_elements(&content, &[
            "button",
            "nav",
            "[data-testid='headerNav']",
            ".js-postMetaLockup",
        ]);
        
        return Ok(content);
    }
    
    Err("Could not extract content".into())
}

fn extract_author(document: &Html) -> Option<String> {
    let selectors = [
        "a[data-testid='authorName']",
        "a[rel='author']",
        "span[data-testid='authorName']",
    ];
    
    for selector_str in &selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                let author = element.text().collect::<String>().trim().to_string();
                if !author.is_empty() {
                    return Some(author);
                }
            }
        }
    }
    
    None
}
```

### Step 3: Register the Extractor

Add your extractor to the registry in `src/extractor.rs`:

```rust
impl ExtractorRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            extractors: Vec::new(),
        };
        
        // Register extractors in priority order
        registry.register(Box::new(MediumExtractor));
        registry.register(Box::new(SubstackExtractor));
        registry.register(Box::new(WikipediaExtractor));
        // Add your new extractor here
        registry.register(Box::new(GenericExtractor)); // Fallback
        
        registry
    }
}
```

### Step 4: Add Tests

Create tests for your extractor:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_can_extract_medium_urls() {
        let extractor = MediumExtractor;
        
        assert!(extractor.can_extract("https://medium.com/@user/article", &[]));
        assert!(extractor.can_extract("https://towardsdatascience.com/article", &[]));
        assert!(!extractor.can_extract("https://example.com", &[]));
    }
    
    #[test]
    fn test_extract_medium_article() {
        let extractor = MediumExtractor;
        let html = include_str!("../../tests/fixtures/medium_article.html");
        
        let result = extractor.extract_from_html(html).unwrap();
        
        assert_eq!(result.title, "Understanding Rust Ownership");
        assert!(result.content.contains("Rust's ownership system"));
        assert_eq!(result.author, Some("Jane Doe".to_string()));
        assert!(result.published_at.is_some());
    }
}
```

## Advanced Techniques

### Using Schema.org Data

```rust
fn can_extract(&self, url: &str, schema_org_data: &[Value]) -> bool {
    // Check URL pattern
    if url.contains("example-news.com") {
        return true;
    }
    
    // Check schema.org type
    for schema in schema_org_data {
        if let Some(type_field) = schema.get("@type") {
            if type_field == "NewsArticle" {
                return true;
            }
        }
    }
    
    false
}
```

### Handling Dynamic Content

For sites with JavaScript-rendered content:

```rust
fn extract_from_html(&self, html: &str) -> Result<ExtractedContent> {
    // Look for JSON-LD data first
    if let Some(json_ld) = extract_json_ld(html) {
        return extract_from_json_ld(json_ld);
    }
    
    // Fall back to HTML parsing
    extract_from_static_html(html)
}

fn extract_json_ld(html: &str) -> Option<Value> {
    let re = regex::Regex::new(r#"<script[^>]*type="application/ld\+json"[^>]*>(.*?)</script>"#).ok()?;
    
    for cap in re.captures_iter(html) {
        if let Ok(json) = serde_json::from_str(&cap[1]) {
            return Some(json);
        }
    }
    
    None
}
```

### Content Scoring

Implement content scoring for better extraction:

```rust
fn score_paragraph(text: &str) -> f32 {
    let mut score = 0.0;
    
    // Length bonus
    let word_count = text.split_whitespace().count();
    score += word_count as f32 * 0.5;
    
    // Punctuation bonus
    let punctuation_count = text.chars().filter(|c| c.is_ascii_punctuation()).count();
    score += punctuation_count as f32 * 2.0;
    
    // Penalty for short paragraphs
    if word_count < 10 {
        score *= 0.5;
    }
    
    // Bonus for common article words
    let article_words = ["however", "therefore", "moreover", "furthermore"];
    for word in &article_words {
        if text.to_lowercase().contains(word) {
            score += 5.0;
        }
    }
    
    score
}
```

## Best Practices

### 1. Fallback Gracefully

Always provide fallbacks when selectors don't match:

```rust
fn extract_title(document: &Html) -> Result<String> {
    // Try primary selector
    if let Some(title) = try_selector(document, "h1.article-title") {
        return Ok(title);
    }
    
    // Try secondary selector
    if let Some(title) = try_selector(document, "h1") {
        return Ok(title);
    }
    
    // Last resort: use page title
    if let Some(title) = try_selector(document, "title") {
        return Ok(title);
    }
    
    Err("No title found".into())
}
```

### 2. Clean Extracted Content

Remove unwanted elements:

```rust
const REMOVAL_SELECTORS: &[&str] = &[
    "script",
    "style",
    "nav",
    ".advertisement",
    ".social-share",
    "[class*='newsletter']",
    "[id*='popup']",
];

fn clean_content(html: &str) -> String {
    let mut document = Html::parse_fragment(html);
    
    for selector_str in REMOVAL_SELECTORS {
        if let Ok(selector) = Selector::parse(selector_str) {
            // Remove matching elements
            // (Implementation details depend on your HTML manipulation library)
        }
    }
    
    document.html()
}
```

### 3. Handle Edge Cases

```rust
impl Extractor for RobustExtractor {
    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent> {
        // Handle empty HTML
        if html.trim().is_empty() {
            return Err("Empty HTML provided".into());
        }
        
        // Handle malformed HTML
        let document = Html::parse_document(html);
        
        // Check if content exists
        if !has_meaningful_content(&document) {
            return Err("No meaningful content found".into());
        }
        
        // Proceed with extraction
        extract_content(&document)
    }
}
```

### 4. Test Thoroughly

Create comprehensive tests:

```rust
#[test]
fn test_extractor_edge_cases() {
    let extractor = MyExtractor;
    
    // Test empty HTML
    assert!(extractor.extract_from_html("").is_err());
    
    // Test HTML without content
    let no_content = "<html><head></head><body></body></html>";
    assert!(extractor.extract_from_html(no_content).is_err());
    
    // Test malformed HTML
    let malformed = "<html><body><p>Unclosed paragraph";
    let result = extractor.extract_from_html(malformed);
    assert!(result.is_ok() || result.is_err()); // Should handle gracefully
}
```

## Debugging Tips

### Enable Debug Logging

```rust
use log::debug;

fn extract_from_html(&self, html: &str) -> Result<ExtractedContent> {
    debug!("Starting extraction for {}", self.name());
    
    let document = Html::parse_document(html);
    debug!("Parsed document with {} nodes", count_nodes(&document));
    
    let title = extract_title(&document)?;
    debug!("Extracted title: {}", title);
    
    // Continue extraction...
}
```

### Inspect Intermediate Results

```rust
#[cfg(debug_assertions)]
fn debug_save_content(stage: &str, content: &str) {
    use std::fs;
    let filename = format!("debug_{}_{}.html", self.name(), stage);
    fs::write(filename, content).ok();
}
```

## Performance Considerations

1. **Avoid Regex in Hot Paths**: Compile regex once and reuse
2. **Limit DOM Traversal**: Use specific selectors rather than broad searches
3. **Stream Large Content**: For very large articles, consider streaming
4. **Cache Selectors**: Parse selectors once during initialization

```rust
pub struct OptimizedExtractor {
    title_selector: Selector,
    content_selector: Selector,
    author_selector: Selector,
}

impl OptimizedExtractor {
    pub fn new() -> Self {
        Self {
            title_selector: Selector::parse("h1.title").unwrap(),
            content_selector: Selector::parse("div.content").unwrap(),
            author_selector: Selector::parse(".author-name").unwrap(),
        }
    }
}
```