//! Error types for Trek

use thiserror::Error;

/// Trek error types
#[derive(Error, Debug)]
pub enum TrekError {
    /// HTML parsing error
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

    /// Generic error
    #[error("Trek error: {0}")]
    Other(String),
}
