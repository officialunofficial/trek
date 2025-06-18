# Usage Examples

This guide provides practical examples of using Trek in various scenarios.

## Basic Usage

### Rust

```rust
use trek_rs::{Trek, TrekOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create Trek instance with default options
    let trek = Trek::new(TrekOptions::default());
    
    // Sample HTML content
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head>
            <title>My Article</title>
            <meta name="author" content="John Doe">
        </head>
        <body>
            <article>
                <h1>Understanding Rust</h1>
                <p>Rust is a systems programming language...</p>
                <p>It provides memory safety without garbage collection...</p>
            </article>
        </body>
        </html>
    "#;
    
    // Extract content
    let result = trek.extract("https://example.com/article", html)?;
    
    println!("Title: {}", result.title);
    println!("Author: {:?}", result.author);
    println!("Content: {}", result.content);
    println!("Word count: {}", result.metadata.word_count);
    
    Ok(())
}
```

### JavaScript/TypeScript

```javascript
import { Trek } from '@officialunofficial/trek';

// Create Trek instance
const trek = new Trek();

// Your HTML content
const html = `
    <!DOCTYPE html>
    <html>
    <head>
        <title>My Article</title>
    </head>
    <body>
        <article>
            <h1>Understanding WebAssembly</h1>
            <p>WebAssembly is a binary instruction format...</p>
        </article>
    </body>
    </html>
`;

// Extract content
const result = trek.extract('https://example.com', html);

console.log('Title:', result.title);
console.log('Content:', result.content);
console.log('Reading time:', result.metadata.readingTimeMinutes, 'minutes');
```

## Advanced Configuration

### Custom Options

```rust
use trek_rs::{Trek, TrekOptions};

let options = TrekOptions {
    include_images: false,        // Skip image extraction
    include_links: true,          // Keep links in content
    min_content_length: 500,      // Require at least 500 chars
    max_content_length: Some(10000), // Limit to 10k chars
};

let trek = Trek::new(options);
```

### TypeScript with Options

```typescript
import { Trek, TrekOptions } from '@officialunofficial/trek';

const options: TrekOptions = {
    includeImages: true,
    includeLinks: false,
    minContentLength: 300,
    maxContentLength: 5000
};

const trek = new Trek(options);
```

## Real-World Examples

### Extracting News Articles

```rust
use trek_rs::Trek;

async fn extract_news_article(url: &str) -> Result<(), Box<dyn std::error::Error>> {
    // Fetch HTML content
    let html = fetch_html(url).await?;
    
    // Extract with Trek
    let trek = Trek::default();
    let result = trek.extract(url, &html)?;
    
    // Check if it's a news article
    if result.content_type == Some("NewsArticle".to_string()) {
        println!("News article detected!");
        println!("Published: {:?}", result.published_at);
    }
    
    // Save to database
    save_article(Article {
        title: result.title,
        content: result.text_content,
        author: result.author,
        published_at: result.published_at,
        url: url.to_string(),
    }).await?;
    
    Ok(())
}
```

### Blog Post Processing

```javascript
async function processBlogPost(url) {
    const trek = new Trek({
        includeImages: true,
        minContentLength: 1000
    });
    
    try {
        // Fetch the HTML
        const response = await fetch(url);
        const html = await response.text();
        
        // Extract content
        const result = trek.extract(url, html);
        
        // Process the content
        return {
            title: result.title,
            content: result.content,
            excerpt: result.excerpt || generateExcerpt(result.textContent),
            author: result.author || 'Anonymous',
            readingTime: Math.ceil(result.metadata.readingTimeMinutes),
            thumbnail: result.metadata.thumbnailUrl
        };
    } catch (error) {
        console.error('Failed to process blog post:', error);
        throw error;
    }
}
```

### Batch Processing

```rust
use trek_rs::Trek;
use rayon::prelude::*;

fn batch_extract(urls: Vec<&str>) -> Vec<Result<TrekResponse, String>> {
    let trek = Trek::default();
    
    urls.par_iter()
        .map(|url| {
            let html = fetch_html_sync(url)
                .map_err(|e| format!("Failed to fetch {}: {}", url, e))?;
            
            trek.extract(url, &html)
                .map_err(|e| format!("Failed to extract {}: {}", url, e))
        })
        .collect()
}
```

### Content Analysis

```typescript
import { Trek } from '@officialunofficial/trek';

class ContentAnalyzer {
    private trek: Trek;
    
    constructor() {
        this.trek = new Trek();
    }
    
    async analyze(url: string, html: string) {
        const result = this.trek.extract(url, html);
        
        return {
            basic: {
                title: result.title,
                author: result.author,
                publishedAt: result.publishedAt,
                language: result.lang
            },
            metrics: {
                wordCount: result.metadata.wordCount,
                readingTime: result.metadata.readingTimeMinutes,
                hasImages: result.content.includes('<img'),
                hasVideos: result.content.includes('<video')
            },
            quality: {
                hasAuthor: !!result.author,
                hasDate: !!result.publishedAt,
                hasStructuredData: result.metadata.schemas.length > 0,
                contentLength: result.textContent.length,
                isLongForm: result.metadata.wordCount > 1000
            },
            metadata: {
                canonical: result.metadata.canonicalUrl,
                siteName: result.siteName,
                domain: result.metadata.domain,
                extractorUsed: result.extractorUsed
            }
        };
    }
}
```

## Error Handling

### Rust Error Handling

```rust
use trek_rs::{Trek, TrekError};

fn safe_extract(url: &str, html: &str) -> Option<String> {
    let trek = Trek::default();
    
    match trek.extract(url, html) {
        Ok(result) => Some(result.content),
        Err(e) => {
            eprintln!("Extraction failed: {}", e);
            
            // Handle specific error types
            match e {
                TrekError::ParseError(_) => {
                    eprintln!("Invalid HTML");
                }
                TrekError::ExtractionError(_) => {
                    eprintln!("Could not find content");
                }
                _ => {
                    eprintln!("Unknown error");
                }
            }
            
            None
        }
    }
}
```

### JavaScript Error Handling

```javascript
import { Trek } from '@officialunofficial/trek';

async function safeExtract(url, html) {
    const trek = new Trek();
    
    try {
        const result = trek.extract(url, html);
        return { success: true, data: result };
    } catch (error) {
        console.error(`Failed to extract content from ${url}:`, error);
        
        // Return partial data if possible
        return {
            success: false,
            error: error.message,
            fallback: {
                title: extractTitleFallback(html),
                content: extractContentFallback(html)
            }
        };
    }
}
```

## Integration Examples

### Express.js Server

```javascript
import express from 'express';
import { Trek } from '@officialunofficial/trek';

const app = express();
const trek = new Trek();

app.post('/extract', express.json(), (req, res) => {
    const { url, html } = req.body;
    
    if (!url || !html) {
        return res.status(400).json({ 
            error: 'Both url and html are required' 
        });
    }
    
    try {
        const result = trek.extract(url, html);
        res.json(result);
    } catch (error) {
        res.status(500).json({ 
            error: 'Extraction failed',
            message: error.message 
        });
    }
});

app.listen(3000, () => {
    console.log('Trek extraction service running on port 3000');
});
```

### Browser Extension

```javascript
// content.js
import { Trek } from '@officialunofficial/trek';

const trek = new Trek({
    includeImages: false,
    maxContentLength: 5000
});

// Extract current page
function extractCurrentPage() {
    const html = document.documentElement.outerHTML;
    const url = window.location.href;
    
    try {
        const result = trek.extract(url, html);
        
        // Send to background script
        chrome.runtime.sendMessage({
            type: 'CONTENT_EXTRACTED',
            data: {
                title: result.title,
                content: result.textContent,
                readingTime: result.metadata.readingTimeMinutes,
                wordCount: result.metadata.wordCount
            }
        });
    } catch (error) {
        console.error('Extraction failed:', error);
    }
}

// Listen for extraction requests
chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
    if (request.type === 'EXTRACT_CONTENT') {
        extractCurrentPage();
    }
});
```