//! Basic example of using Trek

use trek::{Trek, TrekOptions};

fn main() -> color_eyre::Result<()> {
    // Initialize color-eyre for nice error formatting
    color_eyre::install()?;

    // Sample HTML content
    let html = r#"
        <html>
        <head>
            <title>Example Article</title>
            <meta name="description" content="This is an example article">
            <meta name="author" content="John Doe">
        </head>
        <body>
            <header>
                <nav>Navigation menu</nav>
            </header>
            <main>
                <article>
                    <h1>Example Article Title</h1>
                    <p>Published on January 1, 2024</p>
                    <p>This is the first paragraph of the article content.</p>
                    <p>This is the second paragraph with more interesting content.</p>
                </article>
            </main>
            <footer>
                <p>Copyright 2024</p>
            </footer>
        </body>
        </html>
    "#;

    // Create Trek instance with default options
    let options = TrekOptions {
        debug: true,
        url: Some("https://example.com/article".to_string()),
        ..Default::default()
    };

    let trek = Trek::new(options);

    // Parse the HTML
    match trek.parse(html) {
        Ok(result) => {
            println!("Title: {}", result.metadata.title);
            println!("Author: {}", result.metadata.author);
            println!("Word count: {}", result.metadata.word_count);
            println!("Content: {}", result.content);
        }
        Err(e) => {
            eprintln!("Error parsing HTML: {}", e);
        }
    }

    Ok(())
}
