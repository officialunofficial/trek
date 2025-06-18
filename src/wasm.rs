//! WASM bindings for Trek

use crate::{Trek, TrekOptions};
use serde::Serialize;
use serde_wasm_bindgen::from_value;
use wasm_bindgen::prelude::*;

/// WASM wrapper for Trek
#[wasm_bindgen]
pub struct TrekWasm {
    inner: Trek,
}

#[wasm_bindgen]
impl TrekWasm {
    /// Create a new Trek instance
    #[wasm_bindgen(constructor)]
    pub fn new(options: JsValue) -> Result<TrekWasm, JsValue> {
        // Set panic hook for better error messages in wasm
        console_error_panic_hook::set_once();

        // Initialize tracing
        crate::utils::init_tracing();

        let options: TrekOptions = from_value(options)?;
        Ok(TrekWasm {
            inner: Trek::new(options),
        })
    }

    /// Parse HTML and extract content
    #[wasm_bindgen]
    pub fn parse(&self, html: &str) -> Result<JsValue, JsValue> {
        match self.inner.parse(html) {
            Ok(response) => {
                // Use Serializer with object mode to get plain JS objects
                let serializer =
                    serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
                response
                    .serialize(&serializer)
                    .map_err(|e| JsValue::from_str(&e.to_string()))
            }
            Err(e) => Err(JsValue::from_str(&e.to_string())),
        }
    }

    /// Parse HTML asynchronously (for future use with async extractors)
    #[wasm_bindgen]
    pub async fn parse_async(&self, html: &str) -> Result<JsValue, JsValue> {
        // For now, just call the sync version
        self.parse(html)
    }
}

/// Initialize the WASM module
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    crate::utils::init_tracing();
}
