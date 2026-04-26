//! Shared helpers used by the Markdown converter.

use kuchikiki::NodeRef;

/// Get an attribute value from an element node, or `None` if the node is not
/// an element or the attribute is missing.
pub fn attr(node: &NodeRef, name: &str) -> Option<String> {
    let el = node.as_element()?;
    let attrs = el.attributes.borrow();
    attrs.get(name).map(std::string::ToString::to_string)
}

/// Lower-case tag name of an element node, or empty string for non-elements.
pub fn tag_name(node: &NodeRef) -> String {
    node.as_element()
        .map(|e| e.name.local.to_string().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether `node` is an element with the given (case-insensitive) tag.
pub fn is_tag(node: &NodeRef, name: &str) -> bool {
    tag_name(node).eq_ignore_ascii_case(name)
}

/// Whether `node` is an element whose tag matches one of the supplied names.
pub fn is_any_tag(node: &NodeRef, names: &[&str]) -> bool {
    let n = tag_name(node);
    names.iter().any(|t| t.eq_ignore_ascii_case(&n))
}

/// Whether `node` carries one of the given class tokens.
pub fn has_class(node: &NodeRef, class: &str) -> bool {
    let Some(c) = attr(node, "class") else {
        return false;
    };
    c.split_whitespace().any(|t| t == class)
}

/// Whether `node` has *any* of the supplied class tokens.
pub fn has_any_class(node: &NodeRef, classes: &[&str]) -> bool {
    let Some(c) = attr(node, "class") else {
        return false;
    };
    let tokens: Vec<&str> = c.split_whitespace().collect();
    classes.iter().any(|q| tokens.iter().any(|t| t == q))
}

/// True if every child of `node` is a whitespace text node.
#[allow(dead_code)]
pub fn is_blank(node: &NodeRef) -> bool {
    node.text_contents().trim().is_empty()
}

/// True if `node` is an inline-level element by HTML default.
pub fn is_inline_tag(name: &str) -> bool {
    matches!(
        name,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "br"
            | "cite"
            | "code"
            | "data"
            | "dfn"
            | "em"
            | "i"
            | "kbd"
            | "label"
            | "mark"
            | "q"
            | "rp"
            | "rt"
            | "ruby"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "u"
            | "var"
            | "wbr"
            | "del"
            | "ins"
            | "strike"
            | "tt"
            | "img"
    )
}

/// Trim trailing newlines from a buffer in-place.
pub fn trim_end_newlines(s: &mut String) {
    while s.ends_with('\n') {
        s.pop();
    }
}

/// Make sure `out` ends with at least `n` consecutive newlines.
pub fn ensure_trailing_newlines(out: &mut String, n: usize) {
    let mut have = 0usize;
    for c in out.chars().rev() {
        if c == '\n' {
            have += 1;
        } else {
            break;
        }
    }
    while have < n {
        out.push('\n');
        have += 1;
    }
}
