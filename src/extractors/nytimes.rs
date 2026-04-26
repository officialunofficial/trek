//! NYTimes article extractor — port of Defuddle's `nytimes.ts`.
//!
//! Reads `window.__preloadedData` JSON, walks `article.sprinkledBody.content`
//! (or `article.body.content`), and renders block types into HTML. Picks the
//! best image rendition (`superJumbo` > `jumbo` > `articleLarge`).
// AGENT-P2C: Phase 2C news extractor.

use kuchikiki::NodeRef;
use serde_json::Value;

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use crate::extractors::{escape_attr, escape_html, host_matches_suffix, select_all};

/// New York Times (`nytimes.com`) article extractor.
pub struct NytimesExtractor;

impl NytimesExtractor {
    /// Construct a new extractor instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for NytimesExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for NytimesExtractor {
    fn name(&self) -> &'static str {
        "nytimes"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        ctx.url
            .is_some_and(|u| host_matches_suffix(u, "nytimes.com"))
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let article = extract_preload_data(root).ok_or_else(|| ExtractError::Failed {
            name: "nytimes",
            reason: "no __preloadedData article".into(),
        })?;
        let body = article
            .get("sprinkledBody")
            .and_then(|b| b.get("content"))
            .or_else(|| article.get("body").and_then(|b| b.get("content")));
        let blocks = body.and_then(Value::as_array).cloned().unwrap_or_default();
        if blocks.is_empty() {
            return Err(ExtractError::Failed {
                name: "nytimes",
                reason: "empty preloaded body".into(),
            });
        }
        let html = render_blocks(&blocks);

        let title = article
            .get("headline")
            .and_then(|h| h.get("default"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let summary = article
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let published = article
            .get("firstPublished")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let authors = article
            .get("bylines")
            .and_then(|b| b.as_array())
            .and_then(|arr| arr.first())
            .and_then(|first| first.get("creators"))
            .and_then(|c| c.as_array())
            .map(|creators| {
                creators
                    .iter()
                    .filter_map(|c| c.get("displayName").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        Ok(ExtractedContent {
            content_html: html,
            title: if title.is_empty() { None } else { Some(title) },
            author: if authors.is_empty() {
                None
            } else {
                Some(authors)
            },
            site: Some("The New York Times".to_string()),
            published: if published.is_empty() {
                None
            } else {
                Some(published)
            },
            description: if summary.is_empty() {
                None
            } else {
                Some(summary)
            },
            schema_overrides: vec![],
        })
    }
}

fn extract_preload_data(root: &NodeRef) -> Option<Value> {
    let scripts = select_all(root, "script");
    for s in &scripts {
        // skip src= scripts
        if let Some(el) = s.as_element() {
            if el.attributes.borrow().get("src").is_some() {
                continue;
            }
        }
        let text = collect_text(s);
        if !text.contains("__preloadedData") {
            continue;
        }
        let needle = "window.__preloadedData";
        let Some(idx) = text.find(needle) else {
            continue;
        };
        let after = &text[idx + needle.len()..];
        // Find first '{'
        let Some(brace_idx) = after.find('{') else {
            continue;
        };
        let after = &after[brace_idx..];
        // Walk brace pairs respecting strings.
        let raw = match scan_balanced_object(after) {
            Some(s) => s,
            None => continue,
        };
        let cleaned = raw
            .replace(":undefined,", ":null,")
            .replace(":undefined}", ":null}")
            .replace(":undefined]", ":null]");
        match serde_json::from_str::<Value>(&cleaned) {
            Ok(v) => {
                let article = v
                    .get("initialData")
                    .and_then(|d| d.get("data"))
                    .and_then(|d| d.get("article"))
                    .cloned();
                if article.is_some() {
                    return article;
                }
            }
            Err(_e) => {}
        }
    }
    None
}

fn scan_balanced_object(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    let mut start = None;
    for (i, &b) in bytes.iter().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if in_str {
            if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => {
                if depth == 0 {
                    start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let s0 = start?;
                    return Some(s[s0..=i].to_string());
                }
            }
            _ => {}
        }
    }
    let _ = (depth, start);
    None
}

fn collect_text(node: &NodeRef) -> String {
    let mut out = String::new();
    for d in node.descendants() {
        if let Some(t) = d.as_text() {
            out.push_str(&t.borrow());
        }
    }
    out
}

fn render_blocks(blocks: &[Value]) -> String {
    let mut parts = Vec::new();
    for block in blocks {
        let typename = block
            .get("__typename")
            .and_then(Value::as_str)
            .unwrap_or("");
        match typename {
            "ParagraphBlock" => {
                let inner = render_inlines(block.get("content"));
                parts.push(format!("<p>{inner}</p>"));
            }
            "Heading2Block" => {
                parts.push(format!("<h2>{}</h2>", render_inlines(block.get("content"))))
            }
            "Heading3Block" => {
                parts.push(format!("<h3>{}</h3>", render_inlines(block.get("content"))))
            }
            "Heading4Block" => {
                parts.push(format!("<h4>{}</h4>", render_inlines(block.get("content"))))
            }
            "ImageBlock" => {
                if let Some(media) = block.get("media") {
                    if let Some(src) = best_image_url(media) {
                        let alt = media
                            .get("altText")
                            .and_then(Value::as_str)
                            .or_else(|| {
                                media
                                    .get("caption")
                                    .and_then(|c| c.get("text"))
                                    .and_then(Value::as_str)
                            })
                            .unwrap_or_default();
                        let caption = media
                            .get("caption")
                            .and_then(|c| c.get("text"))
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let credit = media
                            .get("credit")
                            .and_then(Value::as_str)
                            .unwrap_or_default();
                        let mut figcap_parts = Vec::new();
                        if !caption.is_empty() {
                            figcap_parts.push(caption.to_string());
                        }
                        if !credit.is_empty() {
                            figcap_parts.push(credit.to_string());
                        }
                        if figcap_parts.is_empty() {
                            parts.push(format!(
                                r#"<img src="{}" alt="{}">"#,
                                escape_attr(&src),
                                escape_attr(alt)
                            ));
                        } else {
                            parts.push(format!(
                                r#"<figure><img src="{}" alt="{}"><figcaption>{}</figcaption></figure>"#,
                                escape_attr(&src),
                                escape_attr(alt),
                                escape_html(&figcap_parts.join(" "))
                            ));
                        }
                    }
                }
            }
            "HeaderBasicBlock" | "Dropzone" => {}
            _ => {
                if block
                    .get("content")
                    .and_then(Value::as_array)
                    .is_some_and(|a| !a.is_empty())
                {
                    parts.push(format!("<p>{}</p>", render_inlines(block.get("content"))));
                }
            }
        }
    }
    parts.join("\n")
}

fn render_inlines(inlines: Option<&Value>) -> String {
    let arr = match inlines.and_then(Value::as_array) {
        Some(a) => a,
        None => return String::new(),
    };
    let mut out = String::new();
    for inl in arr {
        let mut text = escape_html(inl.get("text").and_then(Value::as_str).unwrap_or(""));
        if let Some(formats) = inl.get("formats").and_then(Value::as_array) {
            for fmt in formats {
                let kind = fmt.get("__typename").and_then(Value::as_str).unwrap_or("");
                match kind {
                    "BoldFormat" => text = format!("<strong>{text}</strong>"),
                    "ItalicFormat" => text = format!("<em>{text}</em>"),
                    "LinkFormat" => {
                        if let Some(url) = fmt.get("url").and_then(Value::as_str) {
                            text = format!(r#"<a href="{}">{}</a>"#, escape_attr(url), text);
                        }
                    }
                    _ => {}
                }
            }
        }
        out.push_str(&text);
    }
    out
}

fn best_image_url(media: &Value) -> Option<String> {
    let crops = media.get("crops").and_then(Value::as_array)?;
    for name in ["superJumbo", "jumbo", "articleLarge"] {
        for crop in crops {
            if let Some(rends) = crop.get("renditions").and_then(Value::as_array) {
                for r in rends {
                    if r.get("name").and_then(Value::as_str) == Some(name) {
                        if let Some(url) = r.get("url").and_then(Value::as_str) {
                            return Some(url.to_string());
                        }
                    }
                }
            }
        }
    }
    for crop in crops {
        if let Some(rends) = crop.get("renditions").and_then(Value::as_array) {
            if let Some(first) = rends.first() {
                if let Some(url) = first.get("url").and_then(Value::as_str) {
                    return Some(url.to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::dom::parse_html;

    #[test]
    fn can_extract_url() {
        let e = NytimesExtractor::new();
        let ctx = ExtractCtx::new(Some("https://www.nytimes.com/2024/01/01/us/foo.html"), &[]);
        assert!(e.can_extract(&ctx));
        let ctx2 = ExtractCtx::new(Some("https://example.com"), &[]);
        assert!(!e.can_extract(&ctx2));
    }

    #[test]
    fn render_simple_blocks() {
        let blocks = serde_json::json!([
            {"__typename": "Heading2Block", "content": [{"__typename": "Inline", "text": "Hi"}]},
            {"__typename": "ParagraphBlock", "content": [{"__typename": "Inline", "text": "Body."}]},
        ]);
        let html = render_blocks(blocks.as_array().unwrap());
        assert!(html.contains("<h2>Hi</h2>"));
        assert!(html.contains("<p>Body.</p>"));
    }

    #[test]
    fn extracts_preload_data() {
        let html = r#"<html><body><script>window.__preloadedData = {"initialData":{"data":{"article":{"headline":{"default":"T"},"summary":"S","firstPublished":"2025-01-01","sprinkledBody":{"content":[{"__typename":"ParagraphBlock","content":[{"__typename":"Inline","text":"Hello"}]}]}}}}};</script></body></html>"#;
        let root = parse_html(html);
        let e = NytimesExtractor::new();
        let ctx = ExtractCtx::new(Some("https://nytimes.com/x"), &[]);
        let out = e.extract(&ctx, &root).unwrap();
        assert_eq!(out.title.as_deref(), Some("T"));
        assert!(out.content_html.contains("Hello"));
    }
}
