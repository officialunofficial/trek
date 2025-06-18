# API Reference

## Core Types

### `Trek`

The main extractor instance that handles content extraction.

```rust
pub struct Trek {
    options: TrekOptions,
    registry: ExtractorRegistry,
}
```

#### Methods

##### `new(options: TrekOptions) -> Self`

Creates a new Trek instance with the specified options.

```rust
let trek = Trek::new(TrekOptions::default());
```

##### `extract(&self, url: &str, html: &str) -> Result<TrekResponse>`

Extracts content from the provided HTML.

**Parameters:**
- `url`: The URL of the page (used for site-specific extraction)
- `html`: The HTML content to extract from

**Returns:** `Result<TrekResponse>` containing the extracted content or an error

```rust
let result = trek.extract("https://example.com", html)?;
```

### `TrekOptions`

Configuration options for content extraction.

```rust
pub struct TrekOptions {
    pub include_images: bool,
    pub include_links: bool,
    pub min_content_length: usize,
    pub max_content_length: Option<usize>,
}
```

#### Fields

- `include_images`: Whether to include images in extracted content (default: `true`)
- `include_links`: Whether to preserve links in extracted content (default: `true`)
- `min_content_length`: Minimum content length in characters (default: `200`)
- `max_content_length`: Optional maximum content length

#### Default Implementation

```rust
impl Default for TrekOptions {
    fn default() -> Self {
        Self {
            include_images: true,
            include_links: true,
            min_content_length: 200,
            max_content_length: None,
        }
    }
}
```

### `TrekResponse`

The response containing extracted content and metadata.

```rust
pub struct TrekResponse {
    pub title: String,
    pub content: String,
    pub text_content: String,
    pub excerpt: Option<String>,
    pub author: Option<String>,
    pub site_name: Option<String>,
    pub published_at: Option<String>,
    pub lang: Option<String>,
    pub content_type: Option<String>,
    pub is_mobile_doc: bool,
    pub extractor_used: String,
    pub metadata: TrekMetadata,
}
```

#### Fields

- `title`: The extracted page title
- `content`: HTML content of the main article
- `text_content`: Plain text version of the content
- `excerpt`: Article excerpt or description
- `author`: Article author(s)
- `site_name`: Name of the website
- `published_at`: Publication date
- `lang`: Content language
- `content_type`: MIME type or content classification
- `is_mobile_doc`: Whether this is a mobile-optimized page
- `extractor_used`: Name of the extractor that was used
- `metadata`: Additional metadata

### `TrekMetadata`

Additional metadata extracted from the page.

```rust
pub struct TrekMetadata {
    pub canonical_url: Option<String>,
    pub amp_url: Option<String>,
    pub thumbnail_url: Option<String>,
    pub word_count: usize,
    pub reading_time_minutes: f32,
    pub domain: String,
    pub schemas: Vec<serde_json::Value>,
}
```

## WebAssembly API

### JavaScript/TypeScript Interface

```typescript
interface TrekOptions {
  includeImages?: boolean;
  includeLinks?: boolean;
  minContentLength?: number;
  maxContentLength?: number;
}

interface TrekResponse {
  title: string;
  content: string;
  textContent: string;
  excerpt?: string;
  author?: string;
  siteName?: string;
  publishedAt?: string;
  lang?: string;
  contentType?: string;
  isMobileDoc: boolean;
  extractorUsed: string;
  metadata: {
    canonicalUrl?: string;
    ampUrl?: string;
    thumbnailUrl?: string;
    wordCount: number;
    readingTimeMinutes: number;
    domain: string;
    schemas: any[];
  };
}

class Trek {
  constructor(options?: TrekOptions);
  extract(url: string, html: string): TrekResponse;
}
```

### Usage Example

```javascript
import { Trek } from '@officialunofficial/trek';

const trek = new Trek({
  includeImages: true,
  includeLinks: true,
  minContentLength: 200
});

const result = trek.extract('https://example.com', htmlContent);
console.log(result.title);
console.log(result.content);
```

## Error Handling

Trek uses Rust's `Result` type for error handling. In WebAssembly, errors are thrown as JavaScript exceptions.

### Common Errors

- `ParseError`: HTML parsing failed
- `ExtractionError`: Content extraction failed
- `InvalidInput`: Invalid URL or HTML provided

### Error Example (Rust)

```rust
match trek.extract(url, html) {
    Ok(result) => println!("Title: {}", result.title),
    Err(e) => eprintln!("Extraction failed: {}", e),
}
```

### Error Example (JavaScript)

```javascript
try {
  const result = trek.extract(url, html);
  console.log(result.title);
} catch (error) {
  console.error('Extraction failed:', error);
}
```