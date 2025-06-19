//! Integration tests for Trek

#![allow(clippy::disallowed_methods)] // OK to use unwrap in tests

use trek_rs::{Trek, TrekOptions};

#[test]
fn test_basic_extraction() {
    let html = r#"
        <html>
        <head>
            <title>Test Article</title>
            <meta name="description" content="A test article">
            <meta name="author" content="Test Author">
        </head>
        <body>
            <nav>Navigation</nav>
            <article>
                <h1>Test Article</h1>
                <p>This is the first paragraph with a lot of content to ensure we have enough words. The quick brown fox jumps over the lazy dog. This sentence contains every letter of the alphabet and adds substantial content to our test case.</p>
                <p>This is the second paragraph with even more content. We need to make sure we have at least 200 words in total so that the retry mechanism doesn't kick in. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.</p>
                <p>Here's a third paragraph to add even more content. The goal is to have enough meaningful text that Trek doesn't think this is a low-content page and retry without clutter removal. This should give us well over 200 words of content after extraction.</p>
                <p>And a fourth paragraph for good measure. Testing content extraction is important to ensure that navigation elements, footers, and other non-content elements are properly removed while preserving the main article content. This helps create a clean reading experience.</p>
                <p>One more paragraph to ensure we have well over 200 words. The content extraction algorithm should work correctly when there's sufficient content, avoiding the retry mechanism that would otherwise preserve navigation and footer elements. This additional text pushes us comfortably past the threshold.</p>
            </article>
            <footer>Footer content</footer>
        </body>
        </html>
    "#;

    let trek = Trek::new(TrekOptions::default());
    let result = trek.parse(html).unwrap();

    assert_eq!(result.metadata.title, "Test Article");
    assert_eq!(result.metadata.author, "Test Author");
    assert!(result.content.contains("first paragraph"));
    assert!(result.content.contains("second paragraph"));
    assert!(!result.content.contains("Navigation"));
    assert!(!result.content.contains("Footer content"));
}

#[test]
fn test_metadata_extraction() {
    let html = r#"
        <html>
        <head>
            <title>Meta Test</title>
            <meta property="og:title" content="Open Graph Title">
            <meta property="og:description" content="OG Description">
            <meta property="og:image" content="https://example.com/image.jpg">
            <meta name="author" content="Meta Author">
            <script type="application/ld+json">
            {
                "@context": "https://schema.org",
                "@type": "Article",
                "headline": "Schema Title",
                "author": {
                    "@type": "Person",
                    "name": "Schema Author"
                },
                "datePublished": "2024-01-01"
            }
            </script>
        </head>
        <body>
            <article>
                <p>Content</p>
            </article>
        </body>
        </html>
    "#;

    let trek = Trek::new(TrekOptions::default());
    let result = trek.parse(html).unwrap();

    assert_eq!(result.metadata.title, "Schema Title");
    assert_eq!(result.metadata.author, "Schema Author");
    assert_eq!(result.metadata.published, "2024-01-01");
    assert_eq!(result.metadata.image, "https://example.com/image.jpg");
    assert!(!result.metadata.schema_org_data.is_empty());
}

#[test]
fn test_content_scoring() {
    let html = r#"
        <html>
        <body>
            <div class="navigation">
                <a href="/home">Home</a>
                <a href="/about">About</a>
                <a href="/contact">Contact</a>
            </div>
            <main class="content">
                <h1>Main Article</h1>
                <p>This is a paragraph with substantial content that should be scored highly. We need to ensure there's enough text here so that the extraction doesn't trigger the retry mechanism. The scoring algorithm should properly identify this as the main content area of the page, distinguishing it from navigation and sidebar elements.</p>
                <p>Another paragraph with even more interesting content to ensure proper scoring. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.</p>
                <p>Here's additional content to make sure we exceed the 200-word threshold. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.</p>
                <p>And even more content for good measure. The extraction algorithm should preserve all of this main content while removing the navigation links and sidebar elements. This ensures a clean, readable version of the article without distracting elements.</p>
                <p>Additional paragraph to push word count above 200. When there's sufficient content in the main article area, the extraction algorithm should confidently remove navigation and sidebar elements without needing to retry. This helps ensure consistent behavior across different types of web pages.</p>
                <p>Published on January 1, 2024 by John Doe</p>
            </main>
            <div class="sidebar">
                <h3>Related Links</h3>
                <a href="/link1">Link 1</a>
                <a href="/link2">Link 2</a>
            </div>
        </body>
        </html>
    "#;

    let trek = Trek::new(TrekOptions::default());
    let result = trek.parse(html).unwrap();

    assert!(result.content.contains("Main Article"));
    assert!(result.content.contains("substantial content"));
    assert!(!result.content.contains("Related Links"));
    assert!(result.metadata.word_count > 10);
}

#[test]
fn test_code_block_preservation() {
    let html = r#"
        <html>
        <body>
            <article>
                <h1>Code Example</h1>
                <p>Here's some code:</p>
                <pre><code class="language-rust">
fn main() {
    println!("Hello, world!");
}
                </code></pre>
            </article>
        </body>
        </html>
    "#;

    let trek = Trek::new(TrekOptions::default());
    let result = trek.parse(html).unwrap();

    assert!(result.content.contains("println!"));
    assert!(result.content.contains("<pre>"));
    assert!(result.content.contains("<code"));
}

#[test]
fn test_retry_on_little_content() {
    let html = r#"
        <html>
        <body>
            <div class="ad-container">Advertisement</div>
            <article class="main-content">
                <p>Short content</p>
            </article>
            <div class="social-share">Share buttons</div>
        </body>
        </html>
    "#;

    let mut options = TrekOptions::default();
    options.removal.remove_exact_selectors = true;
    options.removal.remove_partial_selectors = true;

    let trek = Trek::new(options);
    let result = trek.parse(html).unwrap();

    // Should include content even if short
    assert!(result.content.contains("Short content"));
}
