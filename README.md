# Trek

A modern web content extraction library written in Rust, compiled to WebAssembly.

Trek removes clutter from web pages and extracts clean, readable content. It's designed as a modern alternative to Mozilla Readability with enhanced features like mobile-aware extraction and consistent HTML standardization.

## Features

- 🦀 Written in Rust for performance and safety
- 🌐 Compiles to WebAssembly for browser usage
- 📱 Mobile-aware content extraction
- 🎯 Site-specific extractors for popular platforms
- 🔧 Configurable extraction options
- 📊 Content scoring algorithm
- 🏷️ Metadata extraction (title, author, date, etc.)

## Installation

### As a Rust library

```toml
[dependencies]
trek = "0.1"
```

### As a WASM module

```bash
npm install @trek/wasm
```

## Usage

### Rust

```rust
use trek::{Trek, TrekOptions};

let options = TrekOptions {
    debug: false,
    url: Some("https://example.com".to_string()),
    ..Default::default()
};

let trek = Trek::new(options);
let result = trek.parse(html_content)?;

println!("Title: {}", result.metadata.title);
println!("Content: {}", result.content);
```

### Web Playground

Trek includes an interactive web playground for testing content extraction:

```bash
# Build WASM and start the playground server
make playground

# Open http://localhost:8000/playground/ in your browser
```

The playground provides:
- **Live Extraction**: Paste HTML and see extracted content instantly
- **Multiple Views**: Switch between content, metadata, raw JSON, and debug tabs
- **Extraction Options**: Toggle clutter removal and metadata inclusion
- **Example Content**: Pre-loaded example to demonstrate Trek's capabilities

#### Playground Features

- **Content Tab**: Shows the extracted article content with proper formatting
- **Metadata Tab**: Displays title, author, word count, and other metadata
- **Raw JSON Tab**: View the complete extraction response
- **Debug Tab**: See extraction details and performance metrics

### JavaScript/TypeScript

```javascript
import { TrekWasm } from '@trek/wasm';

const trek = new TrekWasm({
    debug: false,
    url: 'https://example.com'
});

const result = await trek.parse(htmlContent);

console.log('Title:', result.title);
console.log('Content:', result.content);
```

## Building

### Native library

```bash
cargo build --release
```

### WebAssembly

```bash
wasm-pack build --target web --out-dir pkg
```

## Development

```bash
# Run tests
cargo test

# Run clippy
cargo clippy --all-targets --all-features

# Format code
cargo fmt

# Generate changelog
git cliff -o CHANGELOG.md
```

## Credits

Trek is a fork of [Defuddle](https://github.com/kepano/defuddle) by [@kepano](https://github.com/kepano), refactored into Rust, adding WebAssembly support, site-specific extractors, and additional features.

## License

MIT
