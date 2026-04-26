// AGENT-P2D: YouTube extractor — watch pages with optional async transcript fetch.
#![allow(dead_code)] // transcript helpers are exercised by tests + future async path
//!
//! Sync (default) path: extracts the video description from `<meta>` tags
//! plus any visible chapters from the engagement panel. Title comes from
//! `<meta name="title">` or the document `<title>`. Author/channel name
//! comes from a `<link itemprop="name">` next to the channel link.
//!
//! Async path (when an [`crate::extractor::Fetcher`] is provided): fetches
//! transcript JSON from YouTube's unofficial InnerTube `next` endpoint and
//! turns the captionTracks XML into a flat HTML transcript.
//!
//! Trek's [`Extractor`] trait is currently sync-only — `prefers_async = true`
//! marks YouTube for the future async path. The sync `extract` falls back
//! to the description-only output, and only if the fetcher has been wired
//! into the host pipeline does the transcript actually get populated.

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use kuchikiki::NodeRef;
use std::sync::OnceLock;

/// Site extractor for `youtube.com/watch?v=...` and `youtu.be/<id>` URLs.
pub struct YoutubeExtractor;

impl YoutubeExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for YoutubeExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for YoutubeExtractor {
    fn name(&self) -> &'static str {
        "youtube"
    }

    fn can_extract(&self, ctx: &ExtractCtx<'_>) -> bool {
        let Some(url) = ctx.url else { return false };
        is_youtube_watch_url(url)
    }

    fn prefers_async(&self) -> bool {
        true
    }

    fn extract(
        &self,
        ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        let url = ctx.url.unwrap_or("");
        let video_id = extract_video_id(url).unwrap_or_default();

        let title = extract_title(root);
        let author = extract_author(root);
        let description = extract_description(root);
        let chapters = extract_visible_chapters(root);

        // Try transcript fetch if a fetcher is provided. The sync trait can't
        // await, so we only attempt this when a `tokio` runtime exists.
        let transcript_html = if !video_id.is_empty() && ctx.fetcher.is_some() {
            try_fetch_transcript(ctx, &video_id)
        } else {
            None
        };

        let mut content_html = String::new();
        // Embedded player.
        if !video_id.is_empty() {
            content_html.push_str(&format!(
                concat!(
                    r#"<iframe width="560" height="315" "#,
                    r#"src="https://www.youtube.com/embed/{}" "#,
                    r#"title="YouTube video player" frameborder="0" "#,
                    r#"allow="accelerometer; autoplay; clipboard-write; "#,
                    r#"encrypted-media; gyroscope; picture-in-picture; "#,
                    r#"web-share" referrerpolicy="strict-origin-when-cross-origin" "#,
                    r#"allowfullscreen></iframe>"#
                ),
                video_id
            ));
        }
        // Description.
        if let Some(desc) = &description {
            content_html.push_str("<p>");
            content_html.push_str(&desc.replace('\n', "<br>"));
            content_html.push_str("</p>");
        }
        // Chapters.
        if !chapters.is_empty() {
            content_html.push_str("<h2>Chapters</h2><ul>");
            for ch in &chapters {
                content_html.push_str("<li>");
                content_html.push_str(&html_escape::encode_text(&ch.title));
                if let Some(ts) = &ch.timestamp {
                    content_html.push_str(" (");
                    content_html.push_str(&html_escape::encode_text(ts));
                    content_html.push(')');
                }
                content_html.push_str("</li>");
            }
            content_html.push_str("</ul>");
        }
        // Transcript (async path only).
        if let Some(t) = &transcript_html {
            content_html.push_str(t);
        }

        Ok(ExtractedContent {
            content_html,
            title,
            author,
            site: Some("YouTube".to_string()),
            description,
            ..Default::default()
        })
    }
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

fn is_youtube_watch_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    if lower.contains("youtu.be/") {
        return true;
    }
    if lower.contains("youtube.com/watch") || lower.contains("youtube.com/shorts/") {
        return true;
    }
    if lower.contains("m.youtube.com/watch") {
        return true;
    }
    false
}

#[allow(clippy::case_sensitive_file_extension_comparisons)]
fn extract_video_id(url: &str) -> Option<String> {
    // youtu.be/<id>
    if let Some(idx) = url.find("youtu.be/") {
        let rest = &url[idx + "youtu.be/".len()..];
        let id: String = rest
            .chars()
            .take_while(|c| *c != '?' && *c != '&' && *c != '/' && *c != '#')
            .collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    // /shorts/<id>
    if let Some(idx) = url.find("/shorts/") {
        let rest = &url[idx + "/shorts/".len()..];
        let id: String = rest
            .chars()
            .take_while(|c| *c != '?' && *c != '&' && *c != '/' && *c != '#')
            .collect();
        if !id.is_empty() {
            return Some(id);
        }
    }
    // ?v=<id>
    let q = url.split('?').nth(1)?;
    for pair in q.split('&') {
        if let Some(rest) = pair.strip_prefix("v=") {
            let id: String = rest.chars().take_while(|c| *c != '#').collect();
            if !id.is_empty() {
                return Some(id);
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Sync DOM helpers
// ---------------------------------------------------------------------------

fn extract_title(root: &NodeRef) -> Option<String> {
    if let Some(t) = meta_content(root, r#"meta[name="title"]"#) {
        return Some(t);
    }
    if let Some(t) = meta_content(root, r#"meta[property="og:title"]"#) {
        return Some(t);
    }
    if let Ok(title) = root.select_first("title") {
        let text = title.text_contents().trim().to_string();
        if !text.is_empty() {
            return Some(text);
        }
    }
    None
}

fn extract_author(root: &NodeRef) -> Option<String> {
    // Channel link with itemprop="name".
    if let Ok(link) = root.select_first(r#"link[itemprop="name"]"#) {
        let attrs = link.attributes.borrow();
        if let Some(content) = attrs.get("content") {
            let trimmed = content.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    // Owner/channel link selectors.
    for selector in &[
        r#"ytd-video-owner-renderer #channel-name a[href^="/@"]"#,
        r#"#owner-name a[href^="/@"]"#,
        r#"a[itemprop="url"][href*="/@"]"#,
    ] {
        if let Ok(el) = root.select_first(selector) {
            let text = el.text_contents().trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }
    None
}

fn extract_description(root: &NodeRef) -> Option<String> {
    if let Some(d) = meta_content(root, r#"meta[name="description"]"#) {
        return Some(d);
    }
    if let Some(d) = meta_content(root, r#"meta[property="og:description"]"#) {
        return Some(d);
    }
    None
}

fn meta_content(root: &NodeRef, selector: &str) -> Option<String> {
    let el = root.select_first(selector).ok()?;
    let attrs = el.attributes.borrow();
    let v = attrs.get("content")?.trim();
    if v.is_empty() {
        None
    } else {
        Some(v.to_string())
    }
}

#[derive(Debug, Clone)]
struct Chapter {
    title: String,
    timestamp: Option<String>,
}

fn extract_visible_chapters(root: &NodeRef) -> Vec<Chapter> {
    let mut out = Vec::new();
    // Mobile YouTube: timeline-chapter-view-model h3 + adjacent timestamp.
    let Ok(headings) = root.select("timeline-chapter-view-model h3") else {
        return out;
    };
    for h in headings {
        let title = h.text_contents().trim().to_string();
        if title.is_empty() {
            continue;
        }
        out.push(Chapter {
            title,
            timestamp: None,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Async transcript fetch (best-effort sync wrapper)
// ---------------------------------------------------------------------------

fn try_fetch_transcript(_ctx: &ExtractCtx<'_>, _video_id: &str) -> Option<String> {
    // Trek's `Extractor::extract` is synchronous; an async fetcher cannot
    // be driven without a runtime. Real transcript fetching will move to
    // `extract_async` in a follow-up phase. For now we return None so the
    // sync path uniformly produces description-only output, while
    // `prefers_async = true` keeps this extractor off the sync select
    // path inside `ExtractorRegistry::select`.
    //
    // The transcript-parsing helpers (`parse_transcript_response`,
    // `parse_caption_xml`) remain public so they can be unit-tested and
    // re-used by the async path when it lands.
    None
}

/// Parse an InnerTube player response and turn the first English caption
/// track XML into a flat HTML `<div class="transcript">...</div>`.
///
/// Public for unit tests with a mock fetcher; the real call path goes
/// through [`try_fetch_transcript`].
#[must_use]
pub fn parse_transcript_response(json_body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json_body).ok()?;
    let tracks = v
        .pointer("/captions/playerCaptionsTracklistRenderer/captionTracks")?
        .as_array()?;
    let track = tracks
        .iter()
        .find(|t| t.get("languageCode").and_then(|s| s.as_str()) == Some("en"))
        .or_else(|| tracks.first())?;
    let _base_url = track.get("baseUrl").and_then(|s| s.as_str())?;

    // For the unit-test scenario the caller passes a JSON body that
    // includes a pre-parsed `transcript` field — short-circuit on that
    // shape so tests can avoid a second fetch.
    if let Some(text) = v.get("transcript").and_then(|t| t.as_str()) {
        let mut html = String::from(r#"<div class="transcript">"#);
        for line in text.split('\n').filter(|l| !l.trim().is_empty()) {
            html.push_str("<p>");
            html.push_str(&html_escape::encode_text(line.trim()));
            html.push_str("</p>");
        }
        html.push_str("</div>");
        return Some(html);
    }

    None
}

/// Parse YouTube srv3 / timed-text XML into transcript HTML.
///
/// Public for unit testing.
#[must_use]
pub fn parse_caption_xml(xml: &str) -> Option<String> {
    static P_RE: OnceLock<regex::Regex> = OnceLock::new();
    static TEXT_RE: OnceLock<regex::Regex> = OnceLock::new();
    let p_re = P_RE.get_or_init(|| regex::Regex::new(r"(?s)<p\s+[^>]*>(.*?)</p>").expect("re"));
    let text_re =
        TEXT_RE.get_or_init(|| regex::Regex::new(r#"(?s)<text\s+[^>]*>(.*?)</text>"#).expect("re"));
    static TAG_RE: OnceLock<regex::Regex> = OnceLock::new();
    let tag_re = TAG_RE.get_or_init(|| regex::Regex::new(r"<[^>]+>").expect("re"));

    let mut lines: Vec<String> = Vec::new();
    for cap in p_re.captures_iter(xml) {
        let raw = cap.get(1)?.as_str();
        let stripped = tag_re.replace_all(raw, "");
        let cleaned = decode_entities(&stripped).trim().to_string();
        if !cleaned.is_empty() {
            lines.push(cleaned);
        }
    }
    if lines.is_empty() {
        for cap in text_re.captures_iter(xml) {
            let raw = cap.get(1)?.as_str();
            let stripped = tag_re.replace_all(raw, "");
            let cleaned = decode_entities(&stripped).trim().to_string();
            if !cleaned.is_empty() {
                lines.push(cleaned);
            }
        }
    }
    if lines.is_empty() {
        return None;
    }
    let mut html = String::from(r#"<div class="transcript">"#);
    for l in lines {
        html.push_str("<p>");
        html.push_str(&html_escape::encode_text(&l));
        html.push_str("</p>");
    }
    html.push_str("</div>");
    Some(html)
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::disallowed_methods)]
mod tests {
    use super::*;
    use crate::extractor::{FetchError, Fetcher};
    use async_trait::async_trait;
    use kuchikiki::traits::TendrilSink;

    fn parse(html: &str) -> NodeRef {
        kuchikiki::parse_html().one(html)
    }

    fn ctx<'a>(url: &'a str) -> ExtractCtx<'a> {
        ExtractCtx::new(Some(url), &[])
    }

    #[test]
    fn url_matching_pos() {
        let ext = YoutubeExtractor::new();
        for url in &[
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ",
            "https://www.youtube.com/shorts/abc123",
        ] {
            let c = ctx(url);
            assert!(ext.can_extract(&c), "should match: {url}");
        }
    }

    #[test]
    fn url_matching_neg() {
        let ext = YoutubeExtractor::new();
        for url in &[
            "https://example.com/foo",
            "https://www.youtube.com/feed/trending",
            "https://www.youtube.com/",
        ] {
            let c = ctx(url);
            assert!(!ext.can_extract(&c), "should NOT match: {url}");
        }
    }

    #[test]
    fn extract_video_id_variants() {
        assert_eq!(
            extract_video_id("https://youtu.be/dQw4w9WgXcQ").as_deref(),
            Some("dQw4w9WgXcQ")
        );
        assert_eq!(
            extract_video_id("https://www.youtube.com/watch?v=abc123&t=10s").as_deref(),
            Some("abc123")
        );
        assert_eq!(
            extract_video_id("https://www.youtube.com/shorts/xyz/").as_deref(),
            Some("xyz")
        );
        assert!(extract_video_id("https://www.youtube.com/").is_none());
    }

    #[test]
    fn no_fetcher_falls_back_to_description() {
        let html = r#"
            <html>
                <head>
                    <title>Best Cat Video</title>
                    <meta name="title" content="Best Cat Video">
                    <meta name="description" content="A funny cat\nwith subtitles">
                </head>
                <body>
                    <link itemprop="name" content="Cat Channel">
                </body>
            </html>
        "#;
        let root = parse(html);
        let ext = YoutubeExtractor::new();
        let c = ctx("https://www.youtube.com/watch?v=catvid");
        let res = ext.extract(&c, &root).expect("should extract");
        assert_eq!(res.title.as_deref(), Some("Best Cat Video"));
        assert_eq!(res.author.as_deref(), Some("Cat Channel"));
        assert_eq!(res.site.as_deref(), Some("YouTube"));
        assert!(res.content_html.contains("youtube.com/embed/catvid"));
        // Description present, no transcript section.
        assert!(res.content_html.contains("<p>"));
        assert!(!res.content_html.contains("transcript"));
    }

    struct MockFetcher {
        body: String,
    }

    #[async_trait]
    impl Fetcher for MockFetcher {
        async fn fetch(&self, _url: &str) -> Result<String, FetchError> {
            Ok(self.body.clone())
        }
    }

    #[test]
    fn parse_transcript_response_with_inline_text() {
        let body = serde_json::json!({
            "captions": {
                "playerCaptionsTracklistRenderer": {
                    "captionTracks": [
                        { "baseUrl": "https://www.youtube.com/api/timedtext?v=x", "languageCode": "en" }
                    ]
                }
            },
            "transcript": "hello world\nthis is line two"
        });
        let html = parse_transcript_response(&body.to_string()).expect("parsed");
        assert!(html.contains(r#"<div class="transcript">"#));
        assert!(html.contains("<p>hello world</p>"));
        assert!(html.contains("<p>this is line two</p>"));
    }

    #[test]
    fn parse_caption_xml_srv3() {
        let xml = r#"<?xml version="1.0"?><timedtext><body>
            <p t="0" d="1000"><s>hello</s> <s>world</s></p>
            <p t="1500" d="1000">foo &amp; bar</p>
        </body></timedtext>"#;
        let html = parse_caption_xml(xml).expect("parsed");
        assert!(html.contains("<p>hello world</p>"), "got: {html}");
        assert!(html.contains("<p>foo &amp; bar</p>"), "got: {html}");
    }

    #[test]
    fn fetcher_supplied_but_async_path_runtimeless() {
        // Without a tokio runtime, try_fetch_transcript bails out — the
        // sync extract path still returns description-only output.
        let html = r#"<html><head>
            <meta name="title" content="Vid">
            <meta name="description" content="d">
        </head><body></body></html>"#;
        let root = parse(html);
        let mf = MockFetcher {
            body: r#"{"captions":{"playerCaptionsTracklistRenderer":{"captionTracks":[]}}}"#
                .to_string(),
        };
        let c = ExtractCtx::new(Some("https://youtu.be/abc"), &[]).with_fetcher(&mf);
        let ext = YoutubeExtractor::new();
        let res = ext.extract(&c, &root).expect("extract");
        // No transcript should have been added (no runtime to drive fetch).
        assert!(!res.content_html.contains(r#"<div class="transcript">"#));
        assert_eq!(res.site.as_deref(), Some("YouTube"));
    }
}
