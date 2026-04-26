//! Markdown escaping helpers.
//!
//! Inline text passing through the converter must escape characters that would
//! otherwise be interpreted as Markdown syntax. Mirrors the subset of
//! Turndown's `escape` rules used by Defuddle (`markdown.ts`).

/// Escape Markdown-significant characters in body text.
///
/// Mirrors the subset of Turndown's escape rules that Defuddle ends up
/// applying:
/// * backslashes escape themselves;
/// * literal backticks need escaping (otherwise a stray `` ` `` opens an
///   inline-code span);
/// * `[` / `]` are escaped to avoid being parsed as link syntax;
/// * `_` between word characters is escaped (prevents `og:site_name` from
///   becoming `og:site` followed by emphasized `name`).
///
/// We intentionally do not escape `*`, `~`, etc. universally — Defuddle
/// inherits Turndown's permissive defaults and only escapes when surrounding
/// context would actually trigger Markdown parsing, and matching that
/// exactly is an open-ended diff battle. The current rules cover the
/// fixtures we currently care about.
#[must_use]
pub fn escape_md_text(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '`' => out.push_str("\\`"),
            '[' => out.push_str("\\["),
            ']' => out.push_str("\\]"),
            '_' => {
                let prev_word = i
                    .checked_sub(1)
                    .and_then(|j| chars.get(j))
                    .is_some_and(|c| c.is_alphanumeric());
                let next_word = chars.get(i + 1).is_some_and(|c| c.is_alphanumeric());
                if prev_word && next_word {
                    out.push_str("\\_");
                } else {
                    out.push('_');
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Escape a string for safe inclusion as the *body* of a Markdown table cell.
///
/// Newlines become a single space (table cells must be one logical line) and
/// pipes are backslash-escaped.
#[must_use]
pub fn escape_table_cell(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '|' => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    // Collapse runs of whitespace introduced by the newline → space mapping.
    let mut collapsed = String::with_capacity(out.len());
    let mut prev_space = false;
    for c in out.chars() {
        if c == ' ' {
            if !prev_space {
                collapsed.push(c);
            }
            prev_space = true;
        } else {
            collapsed.push(c);
            prev_space = false;
        }
    }
    collapsed.trim().to_string()
}

/// Decode HTML entities in attribute values / text fragments.
///
/// kuchikiki returns text content with entities already decoded, so this is
/// only used for attribute strings that we're going to embed back in Markdown
/// output.
#[must_use]
pub fn decode_entities(s: &str) -> String {
    html_escape::decode_html_entities(s).into_owned()
}
