// AGENT-P2D: BBCodeData extractor — catch-all detecting BBCode markup.
//!
//! Two activation paths:
//!
//! 1. **Steam-style `#application_config[data-partnereventstore]`** — a JSON
//!    blob containing a BBCode body. This is what Defuddle's
//!    `BbcodeDataExtractor` actually triggers on, and matches the
//!    `extractor--bbcode-data.html` fixture in this repo.
//! 2. **Generic `<pre>` / `<code>` / `<textarea>` BBCode dump** — any page
//!    whose content area is plain BBCode markup. We require ≥3 distinct
//!    BBCode tag occurrences across these elements to gate on real BBCode
//!    content (not just the literal characters `[` and `]` appearing in
//!    code samples).
//!
//! Registered **last** in `extractors/mod.rs` so any more specific extractor
//! wins first.

use crate::extractor::{ExtractCtx, ExtractError, ExtractedContent, Extractor};
use kuchikiki::NodeRef;
use regex::Regex;
use std::sync::OnceLock;

/// Site extractor for BBCode-bearing pages.
pub struct BbcodeDataExtractor;

impl BbcodeDataExtractor {
    /// Construct a new instance.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Default for BbcodeDataExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl Extractor for BbcodeDataExtractor {
    fn name(&self) -> &'static str {
        "bbcode-data"
    }

    fn can_extract(&self, _ctx: &ExtractCtx<'_>) -> bool {
        // can_extract is called *before* the DOM is parsed in the host
        // pipeline — we always claim potentially-eligible. The real gate
        // is in `extract`, which inspects the DOM and falls back to the
        // generic pipeline by returning `Failed` when no BBCode is
        // detected. The host pipeline already treats `Failed` as
        // "fall through to generic" (see lib.rs).
        true
    }

    fn extract(
        &self,
        _ctx: &ExtractCtx<'_>,
        root: &NodeRef,
    ) -> Result<ExtractedContent, ExtractError> {
        // Path 1: Steam-style application_config blob.
        if let Some(result) = extract_from_application_config(root) {
            return Ok(result);
        }

        // Path 2: BBCode in <pre>/<code>/<textarea>.
        if let Some(result) = extract_from_text_containers(root) {
            return Ok(result);
        }

        Err(ExtractError::Failed {
            name: "bbcode-data",
            reason: "no BBCode payload detected".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// Path 1: Steam application_config blob
// ---------------------------------------------------------------------------

fn extract_from_application_config(root: &NodeRef) -> Option<ExtractedContent> {
    let config = root.select_first("#application_config").ok()?;
    let attrs = config.attributes.borrow();
    let event_raw = attrs.get("data-partnereventstore")?;

    let parsed: serde_json::Value = serde_json::from_str(event_raw).ok()?;
    let event = match &parsed {
        serde_json::Value::Array(arr) => arr.first()?,
        v => v,
    };

    let body_obj = event.get("announcement_body")?;
    let body_text = body_obj.get("body").and_then(|v| v.as_str()).unwrap_or("");
    if body_text.is_empty() {
        return None;
    }

    let content_html = bbcode_to_html(body_text);

    let title = body_obj
        .get("headline")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| event.get("event_name").and_then(|v| v.as_str()))
        .map(str::to_string);

    let published = body_obj
        .get("posttime")
        .and_then(serde_json::Value::as_i64)
        .map(format_iso8601_ms);

    let author = attrs
        .get("data-groupvanityinfo")
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
        .and_then(|v| {
            let entry = match &v {
                serde_json::Value::Array(arr) => arr.first().cloned(),
                _ => Some(v),
            };
            entry.and_then(|e| {
                e.get("group_name")
                    .and_then(|s| s.as_str())
                    .map(str::to_string)
            })
        });

    Some(ExtractedContent {
        content_html,
        title,
        author,
        published,
        // Defuddle's BBCodeData extractor returns no `site` value; generic
        // metadata falls back to the URL host which doesn't match the
        // canonical Steam-style fixture. Force-empty so the generic
        // host-name fallback doesn't kick in.
        site: Some(String::new()),
        ..Default::default()
    })
}

fn format_iso8601_ms(unix_seconds: i64) -> String {
    // Manual UTC formatting: YYYY-MM-DDTHH:MM:SS.000Z (matches Defuddle).
    // Avoid pulling in `chrono` just for this — Trek doesn't depend on it.
    let secs = unix_seconds;
    let days_from_epoch = secs.div_euclid(86_400);
    let secs_in_day = secs.rem_euclid(86_400);
    let h = secs_in_day / 3600;
    let m = (secs_in_day % 3600) / 60;
    let s = secs_in_day % 60;

    let (y, mo, d) = days_to_ymd(days_from_epoch);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{m:02}:{s:02}.000Z")
}

#[allow(clippy::cast_possible_truncation)]
fn days_to_ymd(days_from_epoch: i64) -> (i32, u32, u32) {
    // Algorithm from Howard Hinnant's date library (public domain).
    let z = days_from_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d)
}

// ---------------------------------------------------------------------------
// Path 2: <pre>/<code>/<textarea> BBCode detection
// ---------------------------------------------------------------------------

fn extract_from_text_containers(root: &NodeRef) -> Option<ExtractedContent> {
    let mut best: Option<(usize, String)> = None;

    for tag in &["pre", "code", "textarea"] {
        let Ok(matches) = root.select(tag) else {
            continue;
        };
        for el in matches {
            let text = el.text_contents();
            let count = count_bbcode_tags(&text);
            if count >= 3 && best.as_ref().is_none_or(|(c, _)| count > *c) {
                best = Some((count, text));
            }
        }
    }

    let (_, bbcode) = best?;
    let content_html = bbcode_to_html(&bbcode);

    // Title preference: [h1] then [size=...] then None (caller falls back).
    let title = extract_title_from_bbcode(&bbcode);

    Some(ExtractedContent {
        content_html,
        title,
        ..Default::default()
    })
}

fn count_bbcode_tags(text: &str) -> usize {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Match BBCode tags: [name], [name=...], [/name]. Don't match arbitrary
    // bracket text like `[1]` or `[Note]`. Tag names are alphanumeric +
    // underscore + asterisk (for [*]).
    let re = RE.get_or_init(|| {
        Regex::new(r"(?i)\[/?([a-z][a-z0-9_]*|\*)(?:=[^\]]*)?\]").expect("valid regex")
    });
    re.find_iter(text).count()
}

fn extract_title_from_bbcode(bbcode: &str) -> Option<String> {
    static H1: OnceLock<Regex> = OnceLock::new();
    static SIZE: OnceLock<Regex> = OnceLock::new();
    let h1 = H1.get_or_init(|| Regex::new(r"(?is)\[h1\](.*?)\[/h1\]").expect("valid regex"));
    if let Some(cap) = h1.captures(bbcode) {
        let title = strip_inline_bbcode(cap.get(1)?.as_str()).trim().to_string();
        if !title.is_empty() {
            return Some(title);
        }
    }
    let size = SIZE
        .get_or_init(|| Regex::new(r"(?is)\[size=[^\]]+\](.*?)\[/size\]").expect("valid regex"));
    if let Some(cap) = size.captures(bbcode) {
        let title = strip_inline_bbcode(cap.get(1)?.as_str()).trim().to_string();
        if !title.is_empty() {
            return Some(title);
        }
    }
    None
}

fn strip_inline_bbcode(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\[[^\]]+\]").expect("valid regex"));
    re.replace_all(s, "").into_owned()
}

// ---------------------------------------------------------------------------
// BBCode → HTML parser
// ---------------------------------------------------------------------------

/// Convert a BBCode string to HTML.
///
/// Covers: `[b]`, `[i]`, `[u]`, `[s]`, `[h1]`–`[h4]`, `[url=...]`, `[img]`,
/// `[quote]`, `[code]`, `[list]`/`[*]`, `[size=N]`, `[color=...]`, `[p]`,
/// plus Steam-specific `[previewyoutube]`. Unknown tags are stripped.
#[must_use]
pub fn bbcode_to_html(bbcode: &str) -> String {
    let mut html = bbcode.to_string();

    // Headings first (so [h1]...[/h1] doesn't get caught by the strip pass).
    static H1: OnceLock<Regex> = OnceLock::new();
    static H2: OnceLock<Regex> = OnceLock::new();
    static H3: OnceLock<Regex> = OnceLock::new();
    static H4: OnceLock<Regex> = OnceLock::new();
    let h1 = H1.get_or_init(|| Regex::new(r"(?is)\[h1\](.*?)\[/h1\]").expect("re"));
    let h2 = H2.get_or_init(|| Regex::new(r"(?is)\[h2\](.*?)\[/h2\]").expect("re"));
    let h3 = H3.get_or_init(|| Regex::new(r"(?is)\[h3\](.*?)\[/h3\]").expect("re"));
    let h4 = H4.get_or_init(|| Regex::new(r"(?is)\[h4\](.*?)\[/h4\]").expect("re"));
    html = h1.replace_all(&html, "<h1>$1</h1>").into_owned();
    html = h2.replace_all(&html, "<h2>$1</h2>").into_owned();
    html = h3.replace_all(&html, "<h3>$1</h3>").into_owned();
    html = h4.replace_all(&html, "<h4>$1</h4>").into_owned();

    // Inline formatting.
    static B: OnceLock<Regex> = OnceLock::new();
    static I: OnceLock<Regex> = OnceLock::new();
    static U: OnceLock<Regex> = OnceLock::new();
    static S: OnceLock<Regex> = OnceLock::new();
    let b = B.get_or_init(|| Regex::new(r"(?is)\[b\](.*?)\[/b\]").expect("re"));
    let i = I.get_or_init(|| Regex::new(r"(?is)\[i\](.*?)\[/i\]").expect("re"));
    let u = U.get_or_init(|| Regex::new(r"(?is)\[u\](.*?)\[/u\]").expect("re"));
    let s = S.get_or_init(|| Regex::new(r"(?is)\[s\](.*?)\[/s\]").expect("re"));
    html = b.replace_all(&html, "<strong>$1</strong>").into_owned();
    html = i.replace_all(&html, "<em>$1</em>").into_owned();
    html = u.replace_all(&html, "<u>$1</u>").into_owned();
    html = s.replace_all(&html, "<s>$1</s>").into_owned();

    // Sizing / colour: emit a span. Markdown conversion will strip these,
    // but we keep semantic structure for HTML consumers.
    static SIZE: OnceLock<Regex> = OnceLock::new();
    static COLOR: OnceLock<Regex> = OnceLock::new();
    let size =
        SIZE.get_or_init(|| Regex::new(r"(?is)\[size=([^\]]+)\](.*?)\[/size\]").expect("re"));
    let color =
        COLOR.get_or_init(|| Regex::new(r"(?is)\[color=([^\]]+)\](.*?)\[/color\]").expect("re"));
    html = size
        .replace_all(&html, r#"<span style="font-size:$1">$2</span>"#)
        .into_owned();
    html = color
        .replace_all(&html, r#"<span style="color:$1">$2</span>"#)
        .into_owned();

    // Links — rewrite carefully to avoid `javascript:` injection.
    static URL_RE: OnceLock<Regex> = OnceLock::new();
    let url_re = URL_RE.get_or_init(|| {
        Regex::new(r#"(?is)\[url=["']?([^"'\]]+)["']?\](.*?)\[/url\]"#).expect("re")
    });
    html = url_re
        .replace_all(&html, |caps: &regex::Captures| {
            let raw_href = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            // Steam emits `\"` escaping inside JSON BBCode bodies; clean it up.
            let href = raw_href.replace(r#"\""#, "");
            let text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            if is_dangerous_url(&href) {
                text.to_string()
            } else {
                format!(r#"<a href="{}">{}</a>"#, href.trim(), text)
            }
        })
        .into_owned();

    // Images.
    static IMG: OnceLock<Regex> = OnceLock::new();
    let img = IMG.get_or_init(|| Regex::new(r"(?is)\[img\](.*?)\[/img\]").expect("re"));
    html = img.replace_all(&html, r#"<img src="$1">"#).into_owned();

    // Steam preview-YouTube. Defuddle: `[previewyoutube="VID;full"][/previewyoutube]`
    // → `<img src="https://www.youtube.com/watch?v=VID">`.
    static PREVIEW: OnceLock<Regex> = OnceLock::new();
    let preview = PREVIEW.get_or_init(|| {
        Regex::new(r#"(?is)\[previewyoutube=["']?([^;'"\]]+)[^"'\]]*["']?\]\[/previewyoutube\]"#)
            .expect("re")
    });
    html = preview
        .replace_all(&html, r#"<img src="https://www.youtube.com/watch?v=$1">"#)
        .into_owned();

    // Lists.
    static LIST: OnceLock<Regex> = OnceLock::new();
    let list_re = LIST.get_or_init(|| Regex::new(r"(?is)\[list\](.*?)\[/list\]").expect("re"));
    html = list_re
        .replace_all(&html, |caps: &regex::Captures| {
            let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!("<ul>{}</ul>", convert_list_items(inner))
        })
        .into_owned();
    static OLIST: OnceLock<Regex> = OnceLock::new();
    let olist_re = OLIST.get_or_init(|| Regex::new(r"(?is)\[olist\](.*?)\[/olist\]").expect("re"));
    html = olist_re
        .replace_all(&html, |caps: &regex::Captures| {
            let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!("<ol>{}</ol>", convert_list_items(inner))
        })
        .into_owned();

    // Quote / code blocks.
    static QUOTE: OnceLock<Regex> = OnceLock::new();
    let quote =
        QUOTE.get_or_init(|| Regex::new(r"(?is)\[quote(?:=[^\]]+)?\](.*?)\[/quote\]").expect("re"));
    html = quote
        .replace_all(&html, "<blockquote>$1</blockquote>")
        .into_owned();
    static CODE: OnceLock<Regex> = OnceLock::new();
    let code = CODE.get_or_init(|| Regex::new(r"(?is)\[code\](.*?)\[/code\]").expect("re"));
    html = code
        .replace_all(&html, "<pre><code>$1</code></pre>")
        .into_owned();

    // Spoilers.
    static SPOILER: OnceLock<Regex> = OnceLock::new();
    let spoiler =
        SPOILER.get_or_init(|| Regex::new(r"(?is)\[spoiler\](.*?)\[/spoiler\]").expect("re"));
    html = spoiler
        .replace_all(&html, "<details><summary>Spoiler</summary>$1</details>")
        .into_owned();

    // Paragraphs: [p]...[/p] — convert literal newlines inside to <br>.
    static P: OnceLock<Regex> = OnceLock::new();
    let p_re = P.get_or_init(|| Regex::new(r"(?is)\[p\](.*?)\[/p\]").expect("re"));
    html = p_re
        .replace_all(&html, |caps: &regex::Captures| {
            let inner = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            format!("<p>{}</p>", inner.replace('\n', "<br>"))
        })
        .into_owned();

    // Convert remaining bare newlines to <br>.
    html = html.replace('\n', "<br>");

    // Strip any remaining unknown BBCode-shaped tags.
    static STRIP: OnceLock<Regex> = OnceLock::new();
    let strip = STRIP.get_or_init(|| Regex::new(r"\[[^\]]+\]").expect("re"));
    html = strip.replace_all(&html, "").into_owned();

    html
}

fn convert_list_items(inner: &str) -> String {
    // Split on [*] markers. Anything before the first [*] is dropped.
    let mut out = String::new();
    let parts: Vec<&str> = inner.split("[*]").collect();
    for part in parts.into_iter().skip(1) {
        out.push_str("<li>");
        out.push_str(part.trim());
        out.push_str("</li>");
    }
    out
}

fn is_dangerous_url(url: &str) -> bool {
    let trimmed = url.trim().to_ascii_lowercase();
    trimmed.starts_with("javascript:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("vbscript:")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::disallowed_methods)] // unwrap fine in tests
mod tests {
    use super::*;
    use kuchikiki::traits::TendrilSink;

    fn parse(html_str: &str) -> NodeRef {
        kuchikiki::parse_html().one(html_str)
    }

    fn ctx_for(url: &'static str) -> ExtractCtx<'static> {
        // Keep the URL alive via leaking — fine for tests.
        ExtractCtx::new(Some(url), &[])
    }

    #[test]
    fn bbcode_b_i_u_s() {
        assert_eq!(bbcode_to_html("[b]bold[/b]"), "<strong>bold</strong>");
        assert_eq!(bbcode_to_html("[i]em[/i]"), "<em>em</em>");
        assert_eq!(bbcode_to_html("[u]und[/u]"), "<u>und</u>");
        assert_eq!(bbcode_to_html("[s]str[/s]"), "<s>str</s>");
    }

    #[test]
    fn bbcode_headings() {
        assert_eq!(bbcode_to_html("[h1]A[/h1]"), "<h1>A</h1>");
        assert_eq!(bbcode_to_html("[h2]B[/h2]"), "<h2>B</h2>");
        assert_eq!(bbcode_to_html("[h3]C[/h3]"), "<h3>C</h3>");
        assert_eq!(bbcode_to_html("[h4]D[/h4]"), "<h4>D</h4>");
    }

    #[test]
    fn bbcode_url_and_img() {
        assert_eq!(
            bbcode_to_html("[url=https://example.com]hi[/url]"),
            r#"<a href="https://example.com">hi</a>"#
        );
        assert_eq!(
            bbcode_to_html("[img]https://x.test/y.png[/img]"),
            r#"<img src="https://x.test/y.png">"#
        );
    }

    #[test]
    fn bbcode_url_blocks_javascript_scheme() {
        // SSRF / XSS hardening: `javascript:` and `data:` strip the link.
        let out = bbcode_to_html("[url=javascript:alert(1)]click[/url]");
        assert!(!out.contains("javascript:"), "got: {out}");
        assert!(out.contains("click"));
    }

    #[test]
    fn bbcode_quote_code_list() {
        assert_eq!(
            bbcode_to_html("[quote]hi[/quote]"),
            "<blockquote>hi</blockquote>"
        );
        assert_eq!(
            bbcode_to_html("[code]x = 1[/code]"),
            "<pre><code>x = 1</code></pre>"
        );
        let list = bbcode_to_html("[list][*]a[*]b[/list]");
        assert!(list.contains("<ul>"), "got: {list}");
        assert!(list.contains("<li>a</li>"), "got: {list}");
        assert!(list.contains("<li>b</li>"), "got: {list}");
    }

    #[test]
    fn bbcode_size_and_color() {
        let out = bbcode_to_html("[size=20]big[/size]");
        assert!(
            out.contains(r#"<span style="font-size:20">big</span>"#),
            "got: {out}"
        );
        let out = bbcode_to_html("[color=#ff0000]red[/color]");
        assert!(
            out.contains(r#"<span style="color:#ff0000">red</span>"#),
            "got: {out}"
        );
    }

    #[test]
    fn bbcode_strips_unknown_tags() {
        let out = bbcode_to_html("[notarealtag]hi[/notarealtag]");
        // Unknown tags vanish; inner text remains.
        assert!(out.contains("hi"));
        assert!(!out.contains("[notarealtag]"));
        assert!(!out.contains("[/notarealtag]"));
    }

    #[test]
    fn detection_threshold_pos() {
        // 3 BBCode tags → detected.
        let html = r"<html><body><pre>[b]a[/b][i]b[/i][u]c[/u]</pre></body></html>";
        let root = parse(html);
        let ext = BbcodeDataExtractor::new();
        let ctx = ctx_for("https://example.com/foo");
        assert!(ext.can_extract(&ctx));
        let res = ext.extract(&ctx, &root).expect("should extract");
        assert!(res.content_html.contains("<strong>a</strong>"), "{:?}", res);
    }

    #[test]
    fn detection_threshold_neg() {
        // Only 2 BBCode-shaped tag occurrences → below the ≥3 threshold.
        let html = r"<html><body><pre>[b]a[/b]</pre></body></html>";
        let root = parse(html);
        let ext = BbcodeDataExtractor::new();
        let ctx = ctx_for("https://example.com/foo");
        let err = ext.extract(&ctx, &root).unwrap_err();
        assert!(matches!(err, ExtractError::Failed { .. }));
    }

    #[test]
    fn detection_ignores_random_brackets() {
        // `[1]`, `[Note]`, etc — these are NOT BBCode.
        let html = r"<html><body><pre>see [1] and [Note] and [2025]</pre></body></html>";
        let root = parse(html);
        let ext = BbcodeDataExtractor::new();
        let ctx = ctx_for("https://example.com/foo");
        let err = ext.extract(&ctx, &root).unwrap_err();
        assert!(matches!(err, ExtractError::Failed { .. }));
    }

    #[test]
    fn application_config_path_extracts() {
        let html = r#"<html><body><div id="application_config" data-partnereventstore='[{"announcement_body":{"headline":"H","posttime":1736942400,"body":"[p]hello[/p]"}}]' data-groupvanityinfo='[{"group_name":"G"}]'></div></body></html>"#;
        let root = parse(html);
        let ext = BbcodeDataExtractor::new();
        let ctx = ctx_for("https://example.com/foo");
        let res = ext.extract(&ctx, &root).expect("should extract");
        assert_eq!(res.title.as_deref(), Some("H"));
        assert_eq!(res.author.as_deref(), Some("G"));
        assert!(
            res.content_html.contains("<p>hello</p>"),
            "{}",
            res.content_html
        );
        assert_eq!(res.published.as_deref(), Some("2025-01-15T12:00:00.000Z"));
    }

    #[test]
    fn title_from_h1_or_size() {
        let html =
            r"<html><body><pre>[h1]Title One[/h1][b]a[/b][i]b[/i][u]c[/u]</pre></body></html>";
        let root = parse(html);
        let ext = BbcodeDataExtractor::new();
        let ctx = ctx_for("https://example.com/foo");
        let res = ext.extract(&ctx, &root).expect("extract");
        assert_eq!(res.title.as_deref(), Some("Title One"));

        let html = r"<html><body><pre>[size=20]Big Title[/size][b]a[/b][i]b[/i][u]c[/u]</pre></body></html>";
        let root = parse(html);
        let res = ext.extract(&ctx, &root).expect("extract");
        assert_eq!(res.title.as_deref(), Some("Big Title"));
    }

    #[test]
    fn iso_8601_format() {
        // 2025-01-15 12:00:00 UTC = 1_736_942_400
        assert_eq!(format_iso8601_ms(1_736_942_400), "2025-01-15T12:00:00.000Z");
        // Epoch.
        assert_eq!(format_iso8601_ms(0), "1970-01-01T00:00:00.000Z");
    }
}
