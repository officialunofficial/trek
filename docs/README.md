# Trek Documentation

Trek is a high-performance web content extraction library written in Rust that compiles to WebAssembly. It provides an alternative to Mozilla Readability with enhanced mobile-awareness and site-specific extraction capabilities.

## Table of Contents

- [Getting Started](./getting-started.md)
- [API Reference](./api-reference.md)
- [Architecture Overview](./architecture.md)
- [Usage Examples](./examples.md)
- [Extractor Development Guide](./extractor-guide.md)

## Quick Start

```rust
use trek::{Trek, TrekOptions};

// Create extractor with default options
let trek = Trek::new(TrekOptions::default());

// Extract content from HTML
let result = trek.extract("https://example.com", html_content)?;

println!("Title: {}", result.title);
println!("Content: {}", result.content);
println!("Author: {:?}", result.author);
println!("Published: {:?}", result.published_at);
```

## Features

- **Streaming HTML Processing**: Efficient memory usage with `lol_html`
- **Site-Specific Extractors**: Customized extraction for popular websites
- **Smart Content Detection**: Advanced algorithms for identifying main content
- **Mobile Awareness**: Special handling for AMP and mobile sites
- **WebAssembly Support**: Run in browsers via WASM
- **Extensible Architecture**: Easy to add new site-specific extractors

## Installation

### Rust

Add to your `Cargo.toml`:

```toml
[dependencies]
trek = "0.1.0"
```

### WebAssembly

```bash
npm install @officialunofficial/trek
```

## Development

See [Getting Started](./getting-started.md) for development setup and [Architecture Overview](./architecture.md) for understanding the codebase structure.

## Credits

Trek is a fork of [Defuddle](https://github.com/kepano/defuddle) by [@kepano](https://github.com/kepano), refactored into Rust, adding WebAssembly support, site-specific extractors, and additional features.

## License

MIT
