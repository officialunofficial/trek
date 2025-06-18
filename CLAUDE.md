# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Trek is a modern web content extraction library written in Rust that compiles to WebAssembly. It's designed as an alternative to Mozilla Readability with enhanced mobile-awareness and site-specific extraction capabilities.

## Essential Development Commands

### Building
```bash
# Check compilation
make check

# Build native (debug/release)
make build
make build-release

# Build WebAssembly
make wasm-build        # release build
make wasm-build-debug  # debug build
```

### Testing
```bash
# Run tests
make test              # standard tests
make test-verbose      # with output
make wasm-test         # WebAssembly tests
make bench             # benchmarks
```

### Code Quality
```bash
# Format code
make fmt

# Run linter
make clippy

# Pre-commit checks (fmt, check, clippy, test)
make pre-commit

# Full CI checks
make ci
```

### Development Workflow
```bash
# Install all dev dependencies (wasm-pack, cargo-tarpaulin, etc.)
make install-dev-deps

# Serve WASM test page at http://localhost:8000/test-wasm.html
make serve

# Clean, build everything, run all checks
make release
```

## Architecture Overview

Trek uses a multi-stage content extraction pipeline with the following key components:

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
- **Development server**: `make serve` requires Python 3

## Testing Approach

- Unit tests embedded in source files
- Integration tests in `tests/` directory
- Browser-based WASM testing via `test-wasm.html`
- Always run `make pre-commit` before committing changes