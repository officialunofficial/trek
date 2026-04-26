//! Metadata extraction functionality.
//!
//! Closely mirrors Defuddle's `MetadataExtractor` (see `defuddle/src/metadata.ts`).
//! The goal is byte-exact parity for the four-field JSON preamble
//! (title / author / site / published) emitted by the fixtures harness.

use crate::CollectedData;
use crate::types::{MetaTagItem, MiniAppEmbed, TrekMetadata};
use crate::utils::decode_html_entities;
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use tracing::{debug, instrument};

static TITLE_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"<title[^>]*>(.*?)</title>").expect("Invalid regex"));

/// Months for parseDateText.
const MONTHS: &[(&str, &str)] = &[
    ("january", "01"),
    ("february", "02"),
    ("march", "03"),
    ("april", "04"),
    ("may", "05"),
    ("june", "06"),
    ("july", "07"),
    ("august", "08"),
    ("september", "09"),
    ("october", "10"),
    ("november", "11"),
    ("december", "12"),
];

/// Extract metadata from HTML document
#[derive(Debug)]
pub struct MetadataExtractor;

impl MetadataExtractor {
    /// Extract metadata from collected data
    #[instrument(skip(data, url))]
    pub fn extract_from_collected_data(data: &CollectedData, url: Option<&str>) -> TrekMetadata {
        let mut metadata = TrekMetadata::default();

        // --- Domain ---
        // Trek's API surface includes `metadata.domain`, which is always
        // derived from the supplied URL option for callers' convenience.
        if let Some(u) = url {
            if !u.is_empty() {
                if let Ok(parsed) = url::Url::parse(u) {
                    if let Some(host) = parsed.host_str() {
                        metadata.domain = host.trim_start_matches("www.").to_string();
                    }
                }
            }
        }

        // For the *site* fallback specifically we mirror Defuddle's behavior:
        // `domain` is only used as a last-resort site source when it came
        // from a real document signal (og:url, twitter:url, schema URL, or
        // <link rel="canonical">). The fixtures harness uses `doc.location`
        // when the test supplies a URL, but linkedom (Defuddle's tested
        // backend) does not propagate that into `doc.location.href` — so
        // Defuddle treats the document-level URL as the source of truth.
        let document_domain: String = {
            let candidate = Self::meta_property(&data.meta_tags, "og:url")
                .or_else(|| Self::meta_property(&data.meta_tags, "twitter:url"))
                .or_else(|| Self::schema_property_first(&data.schema_org_data, "url"))
                .or_else(|| data.canonical.clone());
            candidate
                .as_deref()
                .and_then(|u| url::Url::parse(u).ok())
                .and_then(|p| {
                    p.host_str()
                        .map(|h| h.trim_start_matches("www.").to_string())
                })
                .unwrap_or_default()
        };

        // --- Site name ---
        let site_name = Self::get_site_name(&data.schema_org_data, &data.meta_tags);

        // --- Title (with detectedSiteName from cleaning) ---
        // The `<title>` text comes through as the raw inner text (lol_html
        // doesn't decode it), so apply entity decoding here so titles like
        // `Installation Guide &mdash; Example Blog` match Defuddle's
        // `Installation Guide — Example Blog`.
        let doc_title = data.title.as_deref().map(decode_html_entities);
        let best_title = Self::get_best_title(
            &doc_title,
            &data.schema_org_data,
            &data.meta_tags,
            &metadata.domain,
            &site_name,
        );
        let (cleaned_title, detected_site) = Self::clean_title(&best_title, &site_name);
        metadata.title = cleaned_title;

        // --- Author ---
        let author = Self::get_author(&data.schema_org_data, &data.meta_tags);
        metadata.author = author.clone();

        // --- Site (final composition) ---
        // Defuddle: siteName || detectedSiteName || authorAsSite || domain || ''
        // authorAsSite only when author has no comma (single-entity).
        let author_as_site = if !author.is_empty() && !author.contains(',') {
            author.clone()
        } else {
            String::new()
        };
        metadata.site = if !site_name.is_empty() {
            site_name
        } else if !detected_site.is_empty() {
            detected_site
        } else if !author_as_site.is_empty() {
            author_as_site
        } else if !document_domain.is_empty() {
            document_domain
        } else {
            String::new()
        };

        // --- Description ---
        if let Some(d) = Self::get_description(&data.schema_org_data, &data.meta_tags) {
            metadata.description = d;
        }

        // --- Published ---
        if let Some(p) = Self::get_published(&data.schema_org_data, &data.meta_tags) {
            metadata.published = p;
        }

        // --- Image ---
        if let Some(image) = Self::extract_image(&data.schema_org_data, &data.meta_tags) {
            metadata.image = image;
        }

        // --- Favicon ---
        if let Some(favicon) = &data.favicon {
            metadata.favicon.clone_from(favicon);
        }

        metadata.schema_org_data.clone_from(&data.schema_org_data);

        // --- Mini App embed ---
        if let Some(embed_json) = &data.mini_app_embed {
            match serde_json::from_str::<MiniAppEmbed>(embed_json) {
                Ok(embed) => {
                    debug!("Successfully parsed Mini App embed");
                    metadata.mini_app_embed = Some(embed);
                }
                Err(e) => {
                    debug!("Failed to parse Mini App embed JSON: {}", e);
                }
            }
        }

        metadata
    }

    /// Extract metadata from HTML string
    #[instrument(skip(html))]
    pub fn extract(html: &str) -> TrekMetadata {
        let mut metadata = TrekMetadata::default();
        if let Some(captures) = TITLE_PATTERN.captures(html) {
            if let Some(title_match) = captures.get(1) {
                metadata.title = title_match.as_str().trim().to_string();
            }
        }
        metadata
    }

    // -------------------------------------------------------------------
    // helpers
    // -------------------------------------------------------------------

    /// True for unresolved templates (`{{title}}`, `#author.name}`) or strings
    /// that contain no letters/digits at all (`. .`, `-`).
    fn is_placeholder(s: &str) -> bool {
        if s.contains('{') || s.contains('}') {
            return true;
        }
        if let Some(first) = s.chars().next() {
            if first == '#' && s.chars().nth(1).is_some_and(|c| c.is_ascii_alphabetic()) {
                return true;
            }
        }
        !s.chars().any(|c| c.is_alphanumeric())
    }

    /// First non-empty, non-placeholder candidate from a list of thunks.
    fn first_valid(thunks: &[&dyn Fn() -> String]) -> String {
        for thunk in thunks {
            let v = thunk();
            if !v.is_empty() && !Self::is_placeholder(&v) {
                return v;
            }
        }
        String::new()
    }

    fn meta_name(meta_tags: &[MetaTagItem], name: &str) -> Option<String> {
        for tag in meta_tags {
            if tag
                .name
                .as_deref()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
            {
                let v = tag.content.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
        None
    }

    fn meta_property(meta_tags: &[MetaTagItem], property: &str) -> Option<String> {
        for tag in meta_tags {
            if tag
                .property
                .as_deref()
                .is_some_and(|p| p.eq_ignore_ascii_case(property))
            {
                let v = tag.content.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
        None
    }

    /// Collect *all* matching contents (used for citation_author etc.).
    fn meta_names(meta_tags: &[MetaTagItem], name: &str) -> Vec<String> {
        meta_tags
            .iter()
            .filter(|t| {
                t.name
                    .as_deref()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
            .map(|t| t.content.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn meta_properties(meta_tags: &[MetaTagItem], property: &str) -> Vec<String> {
        meta_tags
            .iter()
            .filter(|t| {
                t.property
                    .as_deref()
                    .is_some_and(|p| p.eq_ignore_ascii_case(property))
            })
            .map(|t| t.content.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Walk schema.org data for a dotted property path (e.g. `author.name`,
    /// `publisher.name`, `WebSite.name`). Returns the first matching string
    /// (joined with ", " if many).
    fn schema_property(data: &[Value], path: &str) -> Option<String> {
        let parts: Vec<&str> = path.split('.').collect();

        fn walk(node: &Value, props: &[&str], exact: bool, out: &mut Vec<String>) {
            if props.is_empty() {
                match node {
                    Value::String(s) => out.push(s.clone()),
                    Value::Object(map) => {
                        if let Some(Value::String(name)) = map.get("name") {
                            out.push(name.clone());
                        }
                    }
                    _ => {}
                }
                return;
            }

            match node {
                Value::Array(items) => {
                    let current = props[0];
                    if current.starts_with('[') && current.ends_with(']') {
                        if let Ok(idx) = current[1..current.len() - 1].parse::<usize>() {
                            if let Some(item) = items.get(idx) {
                                walk(item, &props[1..], exact, out);
                            }
                        }
                        return;
                    }
                    // No bracket: try each item.
                    for item in items {
                        walk(item, props, exact, out);
                    }
                }
                Value::Object(map) => {
                    let current = props[0];
                    if let Some(child) = map.get(current) {
                        walk(child, &props[1..], true, out);
                    } else if !exact {
                        for (_, v) in map {
                            if v.is_object() || v.is_array() {
                                walk(v, props, false, out);
                            }
                        }
                    }
                }
                Value::String(s) => {
                    // path-into-string only matches when no remaining parts.
                    if props.is_empty() {
                        out.push(s.clone());
                    }
                }
                _ => {}
            }
        }

        let mut results: Vec<String> = Vec::new();
        for item in data {
            walk(item, &parts, true, &mut results);
        }
        if results.is_empty() {
            for item in data {
                walk(item, &parts, false, &mut results);
            }
        }

        // Dedup while preserving order.
        let mut seen = std::collections::HashSet::new();
        results.retain(|s| !s.trim().is_empty() && seen.insert(s.clone()));
        if results.is_empty() {
            None
        } else {
            Some(results.join(", "))
        }
    }

    fn schema_property_first(data: &[Value], path: &str) -> Option<String> {
        Self::schema_property(data, path)
    }

    // -------------------------------------------------------------------
    // site
    // -------------------------------------------------------------------

    fn get_site_name(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> String {
        let candidate = Self::first_valid(&[
            &|| Self::schema_property(schema_org_data, "publisher.name").unwrap_or_default(),
            &|| Self::meta_property(meta_tags, "og:site_name").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "og:site_name").unwrap_or_default(),
            &|| Self::schema_property(schema_org_data, "WebSite.name").unwrap_or_default(),
            &|| {
                Self::schema_property(schema_org_data, "sourceOrganization.name")
                    .unwrap_or_default()
            },
            &|| Self::meta_name(meta_tags, "copyright").unwrap_or_default(),
            &|| Self::schema_property(schema_org_data, "copyrightHolder.name").unwrap_or_default(),
            &|| Self::schema_property(schema_org_data, "isPartOf.name").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "application-name").unwrap_or_default(),
        ]);

        if candidate.is_empty() {
            return String::new();
        }
        // Reject candidates that are too long to be a real site name.
        if Self::word_count(&candidate) > 6 {
            return String::new();
        }
        candidate
    }

    // -------------------------------------------------------------------
    // title
    // -------------------------------------------------------------------

    fn get_best_title(
        doc_title: &Option<String>,
        schema_org_data: &[Value],
        meta_tags: &[MetaTagItem],
        domain: &str,
        site_name: &str,
    ) -> String {
        let mut candidates: Vec<String> = Vec::new();
        let push = |c: Option<String>, list: &mut Vec<String>| {
            if let Some(s) = c {
                let s = s.trim().to_string();
                if !s.is_empty() && !Self::is_placeholder(&s) {
                    list.push(s);
                }
            }
        };
        push(Self::meta_property(meta_tags, "og:title"), &mut candidates);
        push(Self::meta_name(meta_tags, "twitter:title"), &mut candidates);
        push(
            Self::schema_property(schema_org_data, "headline"),
            &mut candidates,
        );
        push(Self::meta_name(meta_tags, "title"), &mut candidates);
        push(
            Self::meta_name(meta_tags, "sailthru.title"),
            &mut candidates,
        );
        push(doc_title.clone(), &mut candidates);

        if candidates.is_empty() {
            return String::new();
        }

        let author_meta = Self::meta_property(meta_tags, "author")
            .or_else(|| Self::meta_name(meta_tags, "author"))
            .unwrap_or_default();

        let author_norm = author_meta.trim().to_lowercase();
        let site_norm = site_name.trim().to_lowercase();
        let domain_norm = if domain.is_empty() {
            String::new()
        } else {
            // strip leading subdomain stripped is unnecessary; defuddle uses
            // the full domain minus the TLD, then strips non-alphanumerics.
            let stripped: String = {
                if let Some(dot) = domain.rfind('.') {
                    domain[..dot].to_lowercase()
                } else {
                    domain.to_lowercase()
                }
            };
            stripped
                .chars()
                .filter(|c| c.is_ascii_alphanumeric())
                .collect()
        };

        for c in &candidates {
            if !Self::is_site_identifier(c, &author_norm, &site_norm, &domain_norm) {
                return c.clone();
            }
        }
        candidates[0].clone()
    }

    fn is_site_identifier(
        candidate: &str,
        author_norm: &str,
        site_norm: &str,
        domain_norm: &str,
    ) -> bool {
        let norm = candidate.trim().to_lowercase();
        if !author_norm.is_empty() && norm == author_norm {
            return true;
        }
        if !site_norm.is_empty() && norm == site_norm {
            return true;
        }
        if !domain_norm.is_empty() {
            let candidate_norm: String =
                norm.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
            if candidate_norm == *domain_norm {
                return true;
            }
        }
        false
    }

    /// Defuddle's `cleanTitle`: returns (title, detectedSiteName).
    fn clean_title(title: &str, site_name: &str) -> (String, String) {
        if title.is_empty() {
            return (title.to_string(), String::new());
        }

        // Site-name-based removal first.
        if !site_name.is_empty()
            && site_name.to_lowercase() != title.to_lowercase()
            && Self::word_count(site_name) <= 6
        {
            let site_lower = site_name.to_lowercase();
            let escaped = regex::escape(site_name);
            let separators = r"[|\-–—/·]";

            // suffix: `\s*<sep>\s*<site>\s*$`
            let suffix_pat = format!(r"(?i)\s*{separators}\s*{escaped}\s*$");
            if let Ok(re) = Regex::new(&suffix_pat) {
                if re.is_match(title) {
                    let cleaned = re.replace(title, "").trim().to_string();
                    return (cleaned, site_name.to_string());
                }
            }
            // prefix
            let prefix_pat = format!(r"(?i)^\s*{escaped}\s*{separators}\s*");
            if let Ok(re) = Regex::new(&prefix_pat) {
                if re.is_match(title) {
                    let cleaned = re.replace(title, "").trim().to_string();
                    return (cleaned, site_name.to_string());
                }
            }

            // Fuzzy: split on all separators, check if last/first segment is
            // contained in the site name (handles abbreviations like
            // og:site_name="MDN Web Docs" but title `... | MDN`).
            let positions = Self::all_separator_positions(title, r"\s+[|\-–—/·]\s+");
            if !positions.is_empty() {
                // suffix
                let last = *positions.last().unwrap();
                let last_seg = title[last.0 + last.1..].trim().to_lowercase();
                if !last_seg.is_empty() && site_lower.contains(&last_seg) {
                    let mut cut_index = last.0;
                    for i in (0..positions.len() - 1).rev() {
                        let p = positions[i];
                        let segment = title[p.0 + p.1..cut_index].trim();
                        if Self::word_count(segment) > 3 {
                            break;
                        }
                        cut_index = p.0;
                    }
                    return (title[..cut_index].trim().to_string(), site_name.to_string());
                }
                // prefix
                let first = positions[0];
                let prefix_seg = title[..first.0].trim().to_lowercase();
                if !prefix_seg.is_empty() && site_lower.contains(&prefix_seg) {
                    let mut cut_index = first.0 + first.1;
                    for i in 1..positions.len() {
                        let p = positions[i];
                        let segment = title[cut_index..p.0].trim();
                        if Self::word_count(segment) > 3 {
                            break;
                        }
                        cut_index = p.0 + p.1;
                    }
                    return (title[cut_index..].trim().to_string(), site_name.to_string());
                }
            }
        }

        // Heuristic fallback: strong separators (|, /, ·)
        if let Some(out) = Self::try_separator_split(title, r"\s+([|/·])\s+", false, |t, s| {
            s <= 3 && t >= 2 && t >= s * 2
        }) {
            return out;
        }
        // Dash separators (-, –, —)
        if let Some(out) = Self::try_separator_split(title, r"\s+[-–—]\s+", true, |t, s| {
            s <= 2 && t >= 2 && t > s
        }) {
            return out;
        }

        (title.trim().to_string(), String::new())
    }

    fn all_separator_positions(title: &str, pattern: &str) -> Vec<(usize, usize)> {
        let re = match Regex::new(pattern) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        re.find_iter(title)
            .map(|m| (m.start(), m.end() - m.start()))
            .collect()
    }

    fn try_separator_split(
        title: &str,
        pattern: &str,
        suffix_only: bool,
        guard: impl Fn(usize, usize) -> bool,
    ) -> Option<(String, String)> {
        let positions = Self::all_separator_positions(title, pattern);
        if positions.is_empty() {
            return None;
        }

        // suffix
        let last = *positions.last().unwrap();
        let suffix_title = title[..last.0].trim().to_string();
        let suffix_site = title[last.0 + last.1..].trim().to_string();
        if guard(
            Self::word_count(&suffix_title),
            Self::word_count(&suffix_site),
        ) {
            return Some((suffix_title, suffix_site));
        }

        if !suffix_only {
            let first = positions[0];
            let prefix_site = title[..first.0].trim().to_string();
            let prefix_title = title[first.0 + first.1..].trim().to_string();
            if guard(
                Self::word_count(&prefix_title),
                Self::word_count(&prefix_site),
            ) {
                return Some((prefix_title, prefix_site));
            }
        }
        None
    }

    fn word_count(s: &str) -> usize {
        s.split_whitespace().filter(|w| !w.is_empty()).count()
    }

    // -------------------------------------------------------------------
    // author
    // -------------------------------------------------------------------

    fn get_author(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> String {
        // 1. Meta-tag candidates, in Defuddle's priority order.
        let single = Self::first_valid(&[
            &|| Self::meta_name(meta_tags, "sailthru.author").unwrap_or_default(),
            &|| Self::meta_property(meta_tags, "article:author").unwrap_or_default(),
            &|| Self::meta_property(meta_tags, "author").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "author").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "byl").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "authorList").unwrap_or_default(),
        ]);
        if !single.is_empty() {
            // article:author URL guard — Defuddle does NOT skip URL-shaped
            // values here, so we keep cleanAuthorString as-is which strips
            // the URL out of the string. But if the entire string is a URL
            // we'll still emit something — keep current behavior.
            let cleaned = Self::clean_author_string(&single);
            if !cleaned.is_empty() {
                return cleaned;
            }
        }

        // 2. Research paper conventions.
        let mut citation: Vec<String> = Self::meta_names(meta_tags, "citation_author")
            .into_iter()
            .filter(|s| !Self::is_placeholder(s))
            .collect();
        if citation.is_empty() {
            citation = Self::meta_properties(meta_tags, "dc.creator")
                .into_iter()
                .filter(|s| !Self::is_placeholder(s))
                .collect();
        }
        if !citation.is_empty() {
            let parts: Vec<String> = citation
                .iter()
                .map(|s| {
                    if !s.contains(',') {
                        s.trim().to_string()
                    } else {
                        // Convert "Last, First" → "First Last"
                        let parts: Vec<&str> = s.splitn(2, ',').collect();
                        if parts.len() == 2 {
                            format!("{} {}", parts[1].trim(), parts[0].trim())
                        } else {
                            s.trim().to_string()
                        }
                    }
                })
                .collect();
            return parts.join(", ");
        }

        // 3. Schema.org data.
        if let Some(authors) = Self::schema_property(schema_org_data, "author.name")
            .or_else(|| Self::schema_property(schema_org_data, "author.[].name"))
        {
            let parts: Vec<String> = authors
                .split(',')
                .map(|p| p.trim().trim_end_matches(',').trim().to_string())
                .filter(|p| !p.is_empty() && !Self::is_placeholder(p))
                .collect();
            if !parts.is_empty() {
                let mut seen = std::collections::HashSet::new();
                let mut unique: Vec<String> = parts
                    .into_iter()
                    .filter(|p| seen.insert(p.clone()))
                    .collect();
                if unique.len() > 10 {
                    unique.truncate(10);
                }
                return unique.join(", ");
            }
        }

        String::new()
    }

    fn clean_author_string(input: &str) -> String {
        // Static regexes for performance.
        static URL_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(?i)\(?\s*https?://\S+\s*\)?").expect("bad URL regex"));
        static AND_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"(?i),?\s+and\s+").expect("bad AND regex"));
        static TRAILING_SEP_RE: Lazy<Regex> =
            Lazy::new(|| Regex::new(r"\s*[-–—|]\s*$").expect("bad TRAILING regex"));

        let mut s = input.to_string();
        // Strip "By " prefix (case-insensitive).
        if s.is_char_boundary(3) && s[..3].eq_ignore_ascii_case("by ") {
            s = s[3..].to_string();
        }
        // Remove URLs.
        s = URL_RE.replace_all(&s, "").to_string();
        // " and " → ", "
        s = AND_RE.replace_all(&s, ", ").to_string();
        // Trailing separators.
        s = TRAILING_SEP_RE.replace_all(&s, "").to_string();
        s.trim().to_string()
    }

    // -------------------------------------------------------------------
    // description / image / published
    // -------------------------------------------------------------------

    fn get_description(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> Option<String> {
        let v = Self::first_valid(&[
            &|| Self::meta_name(meta_tags, "description").unwrap_or_default(),
            &|| Self::meta_property(meta_tags, "description").unwrap_or_default(),
            &|| Self::meta_property(meta_tags, "og:description").unwrap_or_default(),
            &|| Self::schema_property(schema_org_data, "description").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "twitter:description").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "sailthru.description").unwrap_or_default(),
        ]);
        if v.is_empty() { None } else { Some(v) }
    }

    fn extract_image(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> Option<String> {
        if let Some(v) = Self::meta_property(meta_tags, "og:image") {
            return Some(v);
        }
        if let Some(v) = Self::meta_name(meta_tags, "twitter:image") {
            return Some(v);
        }
        if let Some(v) = Self::schema_property(schema_org_data, "image.url") {
            return Some(v);
        }
        // Schema image as direct string fallback (legacy behaviour).
        for item in schema_org_data {
            if let Some(image) = item.get("image") {
                if let Some(url) = image.as_str() {
                    return Some(url.to_string());
                }
                if let Some(url) = image.get("url").and_then(Value::as_str) {
                    return Some(url.to_string());
                }
                if let Some(arr) = image.as_array() {
                    if let Some(first) = arr.first() {
                        if let Some(url) = first.as_str() {
                            return Some(url.to_string());
                        }
                        if let Some(url) = first.get("url").and_then(Value::as_str) {
                            return Some(url.to_string());
                        }
                    }
                }
            }
        }
        if let Some(v) = Self::meta_name(meta_tags, "sailthru.image.full") {
            return Some(v);
        }
        None
    }

    fn get_published(schema_org_data: &[Value], meta_tags: &[MetaTagItem]) -> Option<String> {
        let v = Self::first_valid(&[
            &|| Self::schema_property(schema_org_data, "datePublished").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "publishDate").unwrap_or_default(),
            &|| Self::meta_property(meta_tags, "article:published_time").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "sailthru.date").unwrap_or_default(),
            &|| Self::meta_name(meta_tags, "publish_date").unwrap_or_default(),
        ]);
        if v.is_empty() { None } else { Some(v) }
    }

    /// Match Defuddle's `parseDateText`: textual dates → ISO8601 with +00:00.
    /// Currently unused at the metadata level (Defuddle uses it for DOM
    /// scraping near the heading) but exposed for future site extractors.
    #[allow(dead_code)]
    pub fn parse_date_text(text: &str) -> Option<String> {
        // "26 February 2025" or "Wednesday, 26 February 2025"
        static DAY_FIRST: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)\b(\d{1,2})\s+(January|February|March|April|May|June|July|August|September|October|November|December)\s+(\d{4})\b").expect("bad regex")
        });
        if let Some(c) = DAY_FIRST.captures(text) {
            let day = format!("{:0>2}", &c[1]);
            let month_name = c[2].to_lowercase();
            let month = MONTHS
                .iter()
                .find(|(n, _)| *n == month_name)
                .map(|(_, m)| *m)?;
            let year = &c[3];
            return Some(format!("{year}-{month}-{day}T00:00:00+00:00"));
        }
        // "February 26, 2025"
        static MONTH_FIRST: Lazy<Regex> = Lazy::new(|| {
            Regex::new(r"(?i)\b(January|February|March|April|May|June|July|August|September|October|November|December)\s+(\d{1,2}),?\s+(\d{4})\b").expect("bad regex")
        });
        if let Some(c) = MONTH_FIRST.captures(text) {
            let month_name = c[1].to_lowercase();
            let month = MONTHS
                .iter()
                .find(|(n, _)| *n == month_name)
                .map(|(_, m)| *m)?;
            let day = format!("{:0>2}", &c[2]);
            let year = &c[3];
            return Some(format!("{year}-{month}-{day}T00:00:00+00:00"));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_title_from_html() {
        let html = r"<html><head><title>Test Title</title></head></html>";
        let metadata = MetadataExtractor::extract(html);
        assert_eq!(metadata.title, "Test Title");
    }

    #[test]
    fn test_clean_title_strips_site_suffix() {
        let (t, d) = MetadataExtractor::clean_title(
            "Article Title - The New York Times",
            "The New York Times",
        );
        assert_eq!(t, "Article Title");
        assert_eq!(d, "The New York Times");
    }

    #[test]
    fn test_clean_title_pipe_strong_sep_with_long_title() {
        // Strong-separator path: title side must be >= 2 words AND >= 2x the
        // site side, mirroring Defuddle's guard. Long titles get stripped.
        let (t, d) = MetadataExtractor::clean_title("How To Cook An Omelette | Foodie", "");
        assert_eq!(t, "How To Cook An Omelette");
        assert_eq!(d, "Foodie");
    }

    #[test]
    fn test_clean_title_em_dash_with_short_site_segment() {
        // Dash separators require sW <= 2 AND tW > sW.
        let (t, d) = MetadataExtractor::clean_title("Hello world wide web — Site", "");
        assert_eq!(t, "Hello world wide web");
        assert_eq!(d, "Site");
    }

    #[test]
    fn test_clean_title_short_pipe_does_not_strip() {
        // 2-word title vs 2-word site shouldn't be stripped (matches Defuddle).
        let (t, _d) = MetadataExtractor::clean_title("Big Story | The Guardian", "");
        assert_eq!(t, "Big Story | The Guardian");
    }

    #[test]
    fn test_clean_title_no_separator() {
        let (t, _d) = MetadataExtractor::clean_title("Just a regular title", "");
        assert_eq!(t, "Just a regular title");
    }

    #[test]
    fn test_clean_author_strips_url_suffix() {
        let cleaned =
            MetadataExtractor::clean_author_string("Dr Jane Smith - https://blog.example.com/");
        assert_eq!(cleaned, "Dr Jane Smith");
    }

    #[test]
    fn test_clean_author_strips_by_prefix() {
        let cleaned = MetadataExtractor::clean_author_string("By Alice Carroll");
        assert_eq!(cleaned, "Alice Carroll");
    }

    #[test]
    fn test_clean_author_normalizes_and_to_comma() {
        let cleaned = MetadataExtractor::clean_author_string("Alice and Bob");
        assert_eq!(cleaned, "Alice, Bob");
    }

    #[test]
    fn test_site_uses_og_site_name_when_present() {
        let mut data = CollectedData::default();
        data.meta_tags.push(MetaTagItem {
            name: None,
            property: Some("og:site_name".to_string()),
            content: "Acme Blog".to_string(),
        });
        let metadata = MetadataExtractor::extract_from_collected_data(
            &data,
            Some("https://www.example.com/article"),
        );
        assert_eq!(metadata.site, "Acme Blog");
        assert_eq!(metadata.domain, "example.com");
    }

    #[test]
    fn test_site_empty_when_no_source_does_not_use_url_host() {
        let mut data = CollectedData::default();
        data.meta_tags.push(MetaTagItem {
            name: Some("description".to_string()),
            property: None,
            content: "desc".to_string(),
        });
        let metadata = MetadataExtractor::extract_from_collected_data(
            &data,
            Some("https://www.example.com/article"),
        );
        // Mirrors Defuddle: the URL/domain does NOT spill into `site`.
        assert_eq!(metadata.site, "");
        assert_eq!(metadata.domain, "example.com");
    }

    #[test]
    fn test_site_falls_back_to_short_author() {
        let mut data = CollectedData::default();
        data.meta_tags.push(MetaTagItem {
            name: Some("author".to_string()),
            property: None,
            content: "Dan Abramov".to_string(),
        });
        let metadata =
            MetadataExtractor::extract_from_collected_data(&data, Some("https://example.com/post"));
        assert_eq!(metadata.author, "Dan Abramov");
        assert_eq!(metadata.site, "Dan Abramov");
    }

    #[test]
    fn test_detected_site_from_title_strip() {
        let mut data = CollectedData::default();
        data.title = Some("Article Title - Example".to_string());
        let metadata =
            MetadataExtractor::extract_from_collected_data(&data, Some("https://example.org/page"));
        // After heuristic strip: title becomes "Article Title" and detected
        // site "Example" backfills metadata.site.
        assert_eq!(metadata.title, "Article Title");
        assert_eq!(metadata.site, "Example");
    }

    #[test]
    fn test_extract_from_collected_data_description() {
        let mut data = CollectedData::default();
        data.meta_tags.push(MetaTagItem {
            name: Some("description".to_string()),
            property: None,
            content: "Test description".to_string(),
        });
        let metadata = MetadataExtractor::extract_from_collected_data(&data, None);
        assert_eq!(metadata.description, "Test description");
    }
}
