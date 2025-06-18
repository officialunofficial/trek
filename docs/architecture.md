# Architecture Overview

Trek is designed with performance, extensibility, and memory efficiency in mind. This document describes the high-level architecture and key design decisions.

## System Architecture

```
┌─────────────────┐     ┌──────────────────┐
│   User Input    │     │  Site-Specific   │
│  (URL + HTML)   │     │    Extractors    │
└────────┬────────┘     └─────────┬────────┘
         │                        │
         ▼                        ▼
┌─────────────────┐     ┌──────────────────┐
│   Trek Core     │────▶│    Extractor     │
│   Orchestrator  │     │    Registry      │
└────────┬────────┘     └──────────────────┘
         │
         ▼
┌─────────────────┐
│ Streaming HTML  │
│     Parser      │
│   (lol_html)    │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Content Extract │
│   & Scoring     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Post-Processing │
│ & Optimization  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Trek Response  │
└─────────────────┘
```

## Core Components

### 1. Trek Core (`src/lib.rs`)

The main orchestrator that coordinates the extraction process.

**Responsibilities:**
- Initialize extraction pipeline
- Manage configuration options
- Coordinate between components
- Handle retry logic

**Key Methods:**
- `extract()`: Main entry point for content extraction
- `new()`: Creates Trek instance with options

### 2. Streaming HTML Parser

Trek uses `lol_html` for streaming HTML processing, which provides:

- **Memory Efficiency**: Process HTML without loading entire DOM
- **Early Data Collection**: Gather metadata during initial pass
- **Selector-Based Processing**: React to specific elements as they stream

**Implementation Details:**
```rust
// Streaming metadata collection
lol_html::HtmlRewriter::new(
    Settings {
        element_content_handlers: vec![
            // Head metadata handlers
            element!("title", title_handler),
            element!("meta", meta_handler),
            // Content handlers
            element!("article, main, [role='main']", content_handler),
        ],
        ..Settings::default()
    },
    |c: &[u8]| output.extend_from_slice(c)
)
```

### 3. Extractor Registry (`src/extractor.rs`)

A registry pattern for managing site-specific extractors.

**Features:**
- Dynamic extractor registration
- Priority-based selection
- Fallback to generic extractor

**Extractor Trait:**
```rust
pub trait Extractor: Send + Sync {
    fn can_extract(&self, url: &str, schema_org_data: &[Value]) -> bool;
    fn extract_from_html(&self, html: &str) -> Result<ExtractedContent>;
    fn name(&self) -> &'static str;
}
```

### 4. Site-Specific Extractors

Located in `src/extractors/`, each extractor is optimized for a specific website or content type.

**Current Extractors:**
- `GenericExtractor`: Default fallback extractor
- `NewsExtractor`: Optimized for news websites
- `BlogExtractor`: Handles blog-style content
- (More can be added following the pattern)

### 5. Content Processing Pipeline

#### Stage 1: Initial Metadata Collection
- Extract title, author, dates
- Collect Open Graph and Twitter Card data
- Identify content type and language

#### Stage 2: Content Extraction
- Use site-specific selectors if available
- Apply generic content detection algorithms
- Score paragraphs based on text density

#### Stage 3: Post-Processing
- Remove clutter (ads, navigation, etc.)
- Standardize HTML structure
- Calculate reading metrics

#### Stage 4: Smart Retry
- If content < min_content_length
- Re-extract without aggressive filtering
- Preserve more borderline content

## Data Flow

1. **Input**: URL and HTML content
2. **Streaming Parse**: First pass to collect metadata
3. **Extractor Selection**: Choose best extractor based on URL/schema
4. **Content Extraction**: Apply extractor to get main content
5. **Enhancement**: Add metadata, calculate metrics
6. **Output**: Structured `TrekResponse`

## Thread Safety

Trek uses `Arc<Mutex<>>` for thread-safe data collection during streaming:

```rust
let metadata = Arc::new(Mutex::new(MetadataCollector::new()));
```

This allows multiple handlers to safely update shared state during HTML streaming.

## Configuration

Key configuration in `src/constants.rs`:

- **REMOVAL_SELECTORS**: Elements to remove during cleanup
- **UNLIKELY_CANDIDATE_REGEX**: Patterns indicating non-content
- **POSITIVE_SCORE_REGEX**: Patterns indicating main content

## WebAssembly Integration

The `src/wasm.rs` module provides:

- JavaScript bindings via `wasm-bindgen`
- Type conversions between Rust and JS
- Error handling across the boundary

## Performance Considerations

1. **Streaming Processing**: Never load full DOM into memory
2. **Early Termination**: Stop processing once enough content found
3. **Regex Caching**: Compile regexes once and reuse
4. **Minimal Allocations**: Reuse buffers where possible

## Extensibility

To add a new extractor:

1. Create new file in `src/extractors/`
2. Implement the `Extractor` trait
3. Register in `ExtractorRegistry::new()`
4. Add tests in `tests/`

## Testing Strategy

- **Unit Tests**: Each module has embedded tests
- **Integration Tests**: Full extraction pipeline tests
- **WASM Tests**: Browser-based testing via test harness
- **Benchmarks**: Performance regression testing