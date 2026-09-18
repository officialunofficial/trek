# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Trek is a library for content extraction from the web. Trek is written in Rust and compiles to WebAssembly. It's designed as an alternative to Mozilla Readability with enhanced mobile-awareness and site-specific extraction capabilities.

## Essential Development Commands

The commands below use `just`. Run `just --list` to see every recipe.

### Building
```bash
# Check compilation
just check

# Build native (debug/release)
just build
just build-release

# Build WebAssembly
just wasm-build        # release build
just wasm-build-debug  # debug build
```

### Testing
```bash
# Run tests
just test              # standard tests
just test-verbose      # with output
just wasm-test         # WebAssembly tests
just bench             # benchmarks
```

### Code Quality
```bash
# Format code
just fmt

# Run linter
just clippy

# Pre-commit checks (fmt, check, clippy, test)
just pre-commit

# Full CI checks
just ci
```

### Development Workflow
```bash
# Install all dev dependencies (wasm-pack, cargo-tarpaulin, etc.)
just install-dev-deps

# Serve WASM test page at http://localhost:8000/test-wasm.html
just serve

# Clean, build everything, run all checks
just release
```

## Architecture Overview

Trek extracts content through a pipeline with several stages. The pipeline has these key components:

### Core Structure
- **`src/lib.rs`**: Main `Trek` struct that orchestrates extraction
- **`src/extractor.rs`**: Registry pattern for site-specific extractors via `Extractor` trait
- **`src/types.rs`**: Core types (`TrekOptions`, `TrekResponse`, `TrekMetadata`)
- **`src/wasm.rs`**: WebAssembly bindings using wasm-bindgen

### Extraction Pipeline
1. **Initial data collection**: Streaming HTML parsing with `lol_html` to gather metadata
2. **Extractor selection**: Check registry for site-specific extractors
3. **Content extraction**: Site-specific or fallback generic extraction
4. **Post-processing**: Clutter removal, standardization, scoring
5. **Smart retry**: Re-extract without clutter removal if content < 200 words

### Key Patterns
- **Streaming HTML processing**: Uses `lol_html` to avoid loading entire DOM
- **Registry pattern**: Extensible site-specific extractors implementing `Extractor` trait
- **Thread-safe data collection**: `Arc<Mutex<>>` for metadata gathering during streaming
- **Configuration-driven**: Removal selectors and options in `constants.rs`

### Adding New Extractors
Implement the `Extractor` trait:
```rust
trait Extractor {
    fn can_extract(&self, url: &str, schema_org_data: &[Value]) -> bool;
    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent>;
    fn name(&self) -> &'static str;
}
```

## Important Notes

- **Rust toolchain**: Uses stable Rust with `wasm32-unknown-unknown` target
- **WASM builds**: Require special RUSTFLAGS: `--cfg getrandom_backend="js"`
- **Strict linting**: Clippy pedantic and nursery lints enabled
- **Size optimization**: Release builds use `opt-level = "z"` for minimal WASM size
- **Development server**: `just serve` requires Python 3

## Testing Approach

- Unit tests embedded in source files
- Integration tests in `tests/` directory
- Browser-based WASM testing via `test-wasm.html`
- Always run `just pre-commit` before committing changes