//! Resolve relative `href`/`src` URLs against the page URL stored in
//! `DomCtx`. If no URL is available the pass is a no-op.

use kuchikiki::NodeRef;
use url::Url;

use crate::dom::walk::is_any_tag;
use crate::dom::{DomCtx, DomPass};

pub struct RelativeUrls;

fn rewrite_attr(node: &NodeRef, attr: &str, base: &Url) {
    let Some(el) = node.as_element() else { return };
    let mut attrs = el.attributes.borrow_mut();
    if let Some(val) = attrs.get(attr).map(std::string::ToString::to_string) {
        let trimmed = val.trim();
        // Skip absolute, anchors, data:, javascript:, mailto:, etc.
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with("data:")
            || trimmed.starts_with("javascript:")
            || trimmed.starts_with("mailto:")
            || trimmed.starts_with("tel:")
        {
            return;
        }
        if let Ok(parsed) = Url::parse(trimmed) {
            let _ = parsed; // already absolute
            return;
        }
        if let Ok(resolved) = base.join(trimmed) {
            attrs.insert(attr, resolved.to_string());
        }
    }
}

impl DomPass for RelativeUrls {
    fn name(&self) -> &'static str {
        "relative_urls"
    }

    fn run(&self, root: &NodeRef, ctx: &DomCtx) {
        let Some(url_str) = ctx.url else { return };
        let Ok(base) = Url::parse(url_str) else {
            return;
        };

        for d in root.descendants() {
            if is_any_tag(&d, &["a", "link"]) {
                rewrite_attr(&d, "href", &base);
            }
            if is_any_tag(&d, &["img", "video", "audio", "source", "iframe", "script"]) {
                rewrite_attr(&d, "src", &base);
            }
        }
    }
}
