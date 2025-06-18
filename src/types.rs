//! Type definitions for Trek

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Options for Trek parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrekOptions {
    /// Enable debug logging
    pub debug: bool,

    /// URL of the page being parsed
    pub url: Option<String>,

    /// Output format options
    #[serde(flatten)]
    pub output: OutputOptions,

    /// Content removal options
    #[serde(flatten)]
    pub removal: RemovalOptions,
}

/// Output format options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputOptions {
    /// Convert output to Markdown
    pub markdown: bool,

    /// Include Markdown in the response
    pub separate_markdown: bool,
}

/// Content removal options
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalOptions {
    /// Whether to remove elements matching exact selectors
    pub remove_exact_selectors: bool,

    /// Whether to remove elements matching partial selectors
    pub remove_partial_selectors: bool,
}

impl Default for TrekOptions {
    fn default() -> Self {
        Self {
            debug: false,
            url: None,
            output: OutputOptions {
                markdown: false,
                separate_markdown: false,
            },
            removal: RemovalOptions {
                remove_exact_selectors: true,
                remove_partial_selectors: true,
            },
        }
    }
}

/// Metadata extracted from the document
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrekMetadata {
    /// Page title
    pub title: String,

    /// Page description
    pub description: String,

    /// Domain of the page
    pub domain: String,

    /// Favicon URL
    pub favicon: String,

    /// Main image URL
    pub image: String,

    /// Parse time in milliseconds
    pub parse_time: u64,

    /// Published date
    pub published: String,

    /// Author name
    pub author: String,

    /// Site name
    pub site: String,

    /// Schema.org data
    pub schema_org_data: Vec<serde_json::Value>,

    /// Word count
    pub word_count: usize,
}

/// Meta tag information
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetaTagItem {
    /// Meta tag name attribute
    pub name: Option<String>,

    /// Meta tag property attribute
    pub property: Option<String>,

    /// Meta tag content
    pub content: String,
}

/// Response from Trek parsing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrekResponse {
    /// Extracted HTML content
    pub content: String,

    /// Extracted content as Markdown (if requested)
    pub content_markdown: Option<String>,

    /// Type of extractor used (if any)
    pub extractor_type: Option<String>,

    /// Meta tags found in the document
    pub meta_tags: Vec<MetaTagItem>,

    /// All metadata fields
    #[serde(flatten)]
    pub metadata: TrekMetadata,
}

/// Variables extracted by site-specific extractors
pub type ExtractorVariables = HashMap<String, String>;

/// Content extracted by site-specific extractors
#[derive(Debug, Clone, Default)]
pub struct ExtractedContent {
    /// Title override
    pub title: Option<String>,

    /// Author override
    pub author: Option<String>,

    /// Published date override
    pub published: Option<String>,

    /// Text content
    pub content: Option<String>,

    /// HTML content
    pub content_html: Option<String>,

    /// Additional variables
    pub variables: Option<ExtractorVariables>,
}
