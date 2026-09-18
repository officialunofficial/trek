//! Error types for Trek

use thiserror::Error;

/// Trek error types.
///
/// This is the public error type for [`crate::Trek::parse`] and
/// [`crate::Trek::parse_html_internal`]. Callers match on a variant to tell
/// failure kinds apart instead of inspecting an opaque error message.
#[derive(Error, Debug)]
pub enum TrekError {
    /// HTML parsing/rewriting error. Covers streaming-parser failures
    /// (`lol_html`) during metadata collection or clutter removal.
    #[error("Failed to parse HTML: {0}")]
    HtmlParse(String),

    /// DOM manipulation error
    #[error("DOM manipulation error: {0}")]
    DomError(String),

    /// Selector parsing error
    #[error("Invalid CSS selector: {0}")]
    SelectorError(String),

    /// Content extraction error
    #[error("Failed to extract content: {0}")]
    ExtractionError(String),

    /// WASM-specific error
    #[cfg(target_arch = "wasm32")]
    #[error("WASM error: {0}")]
    WasmError(String),

    /// The re-entrant parse pipeline ([`crate::Trek::parse_html_internal`])
    /// was called more times than [`crate::extractor::RecursionDepth::DEFAULT_MAX`]
    /// allows, e.g. a site extractor unwrapping nested quote content ad
    /// infinitum.
    #[error("recursion depth exceeded (max {max})")]
    RecursionLimit {
        /// Configured cap that was exceeded.
        max: u32,
    },

    /// Generic error
    #[error("Trek error: {0}")]
    Other(String),
}

impl From<lol_html::errors::RewritingError> for TrekError {
    fn from(err: lol_html::errors::RewritingError) -> Self {
        Self::HtmlParse(err.to_string())
    }
}

impl From<crate::extractor::ExtractError> for TrekError {
    fn from(err: crate::extractor::ExtractError) -> Self {
        match err {
            crate::extractor::ExtractError::RecursionLimit { max } => Self::RecursionLimit { max },
            other => Self::ExtractionError(other.to_string()),
        }
    }
}
