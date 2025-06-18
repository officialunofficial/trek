//! Utility functions for Trek

use tracing::instrument;

/// Get current time in milliseconds (cross-platform)
pub fn current_time_ms() -> u64 {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .try_into()
            .unwrap_or(u64::MAX)
    }

    #[cfg(target_arch = "wasm32")]
    {
        // In WASM, use JavaScript's Date.now()
        js_sys::Date::now() as u64
    }
}

/// Initialize tracing for the library
pub fn init_tracing() {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

        tracing_subscriber::registry()
            .with(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| "trek=info".into()),
            )
            .with(tracing_subscriber::fmt::layer())
            .init();
    }

    #[cfg(target_arch = "wasm32")]
    {
        use std::sync::Once;
        static INIT: Once = Once::new();

        INIT.call_once(|| {
            // For WASM, we use a simpler initialization
            console_error_panic_hook::set_once();
            let _ = tracing_wasm::try_set_as_global_default();
        });
    }
}

/// Count words in HTML content
#[instrument]
pub fn count_words(html: &str) -> usize {
    // Strip HTML tags and count words
    let text = strip_html_tags(html);

    text.split_whitespace()
        .filter(|word| !word.is_empty())
        .count()
}

use once_cell::sync::Lazy;

static TAG_PATTERN: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"<[^>]+>").expect("Invalid regex pattern"));

static WHITESPACE_PATTERN: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"\s+").expect("Invalid regex pattern"));

/// Strip HTML tags from text and normalize whitespace
pub fn strip_html_tags(html: &str) -> String {
    // First remove all HTML tags
    let without_tags = TAG_PATTERN.replace_all(html, " ");

    // Then normalize whitespace (multiple spaces/newlines to single space)
    let normalized = WHITESPACE_PATTERN.replace_all(&without_tags, " ");

    normalized.trim().to_string()
}

/// Decode HTML entities
#[instrument]
pub fn decode_html_entities(text: &str) -> String {
    html_escape::decode_html_entities(text).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_words() {
        let html = "<p>Hello world from Trek!</p>";
        assert_eq!(count_words(html), 4);
    }

    #[test]
    fn test_decode_html_entities() {
        let encoded = "&lt;p&gt;Hello &amp; goodbye&lt;/p&gt;";
        let decoded = decode_html_entities(encoded);
        assert_eq!(decoded, "<p>Hello & goodbye</p>");
    }

    #[test]
    fn test_strip_html_tags() {
        let html = "<p>Hello <strong>world</strong>!</p>";
        let text = strip_html_tags(html);
        // Note: The standardization process now collapses multiple spaces
        assert_eq!(text.trim(), "Hello world !");
    }
}
