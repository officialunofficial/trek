//! Farcaster Mini App extractor

use crate::extractor::Extractor;
use crate::types::ExtractedContent;
use eyre::Result;
use serde_json::Value;
use tracing::debug;

/// Extractor for Farcaster Mini Apps
pub struct FarcasterExtractor;

impl Extractor for FarcasterExtractor {
    fn can_extract(&self, url: &str, _schema_org_data: &[Value]) -> bool {
        // Check if URL matches known Farcaster mini app domains
        url.contains("crowdfund.seedclub.com")
            || url.contains("yoink.party")
            || url.contains("farcaster.xyz")
            || url.contains("warpcast.com")
    }

    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent> {
        debug!("Using Farcaster extractor");

        let mut content = ExtractedContent::default();

        // For Farcaster mini apps, we mainly rely on the metadata
        // The content itself is usually minimal as it's meant to be displayed in a frame

        // Try to extract any meaningful content from the page
        // This is often minimal for mini apps
        if let Some(body_start) = html.find("<body") {
            if let Some(body_end) = html.rfind("</body>") {
                let body_start_tag_end = html[body_start..].find('>').unwrap_or(0) + body_start + 1;
                let body_content = &html[body_start_tag_end..body_end];

                // Clean up the content
                let clean_content = body_content
                    .replace(['\n', '\r', '\t'], " ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");

                if !clean_content.is_empty() {
                    content.content = Some(clean_content.clone());
                    content.content_html = Some(format!("<div>{clean_content}</div>"));
                }
            }
        }

        Ok(content)
    }

    fn name(&self) -> &'static str {
        "farcaster"
    }
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;

    #[test]
    fn test_can_extract() {
        let extractor = FarcasterExtractor;

        assert!(extractor.can_extract("https://crowdfund.seedclub.com/c/123", &[]));
        assert!(extractor.can_extract("https://yoink.party/framesV2", &[]));
        assert!(!extractor.can_extract("https://example.com", &[]));
    }
}
