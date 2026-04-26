//! Run Trek against an HTML file from disk and print results as JSON.
//!
//! Usage: cargo run --example extract_file -- <path-to-html> [url]

use std::env;
use std::fs;

use trek_rs::{Trek, TrekOptions};

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: extract_file <html-path> [url]");
        std::process::exit(2);
    }
    let path = &args[1];
    let url = args.get(2).cloned();

    let html = fs::read_to_string(path)?;

    let options = TrekOptions {
        debug: false,
        url,
        output: trek_rs::types::OutputOptions {
            markdown: true,
            separate_markdown: true,
        },
        ..Default::default()
    };

    let trek = Trek::new(options);
    let response = trek.parse(&html)?;

    // Print structured summary as JSON
    let summary = serde_json::json!({
        "title": response.metadata.title,
        "author": response.metadata.author,
        "site": response.metadata.site,
        "published": response.metadata.published,
        "domain": response.metadata.domain,
        "description": response.metadata.description,
        "image": response.metadata.image,
        "word_count": response.metadata.word_count,
        "extractor_type": response.extractor_type,
        "content_html_length": response.content.len(),
        "content_markdown": response.content_markdown,
        "content_html_first_2k": response.content.chars().take(2000).collect::<String>(),
    });

    println!("{}", serde_json::to_string_pretty(&summary)?);

    Ok(())
}
