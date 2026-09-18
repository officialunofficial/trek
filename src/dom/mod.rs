//! DOM-based post-processing substrate built on the vendored [`engine`].
//!
//! Trek's primary extraction pipeline is streaming and lol_html-based. Some
//! Defuddle-parity rules need a real tree instead. These rules include
//! parent/sibling traversal, scoring across siblings, and complex
//! link-density math. This module provides a thin wrapper over the DOM
//! engine. It also provides a `DomPass` trait. The trait lets us register
//! tree-walking passes that run after the streaming pipeline.
//!
//! The plumbing intentionally lands with no passes registered. Adding the
//! empty pass list MUST be a no-op for existing extraction output.

/// A vendored HTML tree engine. See the module's own doc comment for the
/// migration history and license attribution.
pub mod engine;
pub mod walk;

/// Context passed to every `DomPass` invocation.
#[derive(Debug, Clone, Copy)]
pub struct DomCtx<'a> {
    /// URL of the page being parsed, when known.
    pub url: Option<&'a str>,
    /// Whether to keep debug-only attributes / log verbosely.
    pub debug: bool,
}

impl<'a> DomCtx<'a> {
    /// Construct a new context.
    #[must_use]
    pub const fn new(url: Option<&'a str>, debug: bool) -> Self {
        Self { url, debug }
    }
}

/// A single tree-walking pass over the parsed DOM.
///
/// Passes mutate the tree in-place and are invoked in registration order.
pub trait DomPass {
    /// Stable identifier used for tracing/debugging.
    fn name(&self) -> &'static str;

    /// Run the pass against `root`.
    fn run(&self, root: &crate::dom::engine::NodeRef, ctx: &DomCtx);
}

/// Parse `html` into a document node.
///
/// This is the crate's single parse entry point — every call site should
/// go through here rather than calling `crate::dom::engine::parse_html`
/// directly, so a future change to the parser only needs to be handled in
/// one place.
#[must_use]
pub fn parse_html(html: &str) -> crate::dom::engine::NodeRef {
    use crate::dom::engine::traits::TendrilSink;
    // `Sink::finish` returns the `Sink` itself (not a bare `NodeRef`), so
    // pull the parsed document out of it explicitly.
    crate::dom::engine::parse_html().one(html).document_node
}

/// Serialize a document node back to an HTML string.
///
/// For `Document` nodes this includes `<!DOCTYPE html><html>...` boilerplate;
/// for fragment use cases callers should serialize a specific child instead.
#[must_use]
pub fn serialize(node: &crate::dom::engine::NodeRef) -> String {
    let mut buf: Vec<u8> = Vec::new();
    // The engine serializes to anything implementing std::io::Write.
    if node.serialize(&mut buf).is_ok() {
        String::from_utf8_lossy(&buf).into_owned()
    } else {
        String::new()
    }
}

/// Run a sequence of passes against `html` and return the serialized result.
///
/// With an empty `passes` slice this is `parse → serialize` — the html5ever
/// round-trip will canonicalize markup but should not change semantic
/// content. Callers that need byte-for-byte preservation should skip the
/// substrate when there are no passes; see `Trek::run_dom_passes`.
#[must_use]
pub fn run_passes(html: &str, ctx: &DomCtx, passes: &[Box<dyn DomPass>]) -> String {
    if passes.is_empty() {
        // No-op fast path — avoid the html5ever round-trip entirely so the
        // empty pass list is guaranteed to be byte-for-byte identical to the
        // input.
        return html.to_string();
    }

    let root = parse_html(html);
    for pass in passes {
        pass.run(&root, ctx);
    }
    serialize(&root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_pass_list_is_noop() {
        let html = "<div><p>hello</p></div>";
        let ctx = DomCtx::new(None, false);
        let out = run_passes(html, &ctx, &[]);
        assert_eq!(out, html, "empty pass list must not modify html");
    }

    #[test]
    fn parse_and_serialize_roundtrip_preserves_text() {
        let html = "<html><body><p>hi</p></body></html>";
        let root = parse_html(html);
        let out = serialize(&root);
        assert!(out.contains("<p>hi</p>"));
    }

    struct TagRenamer;
    impl DomPass for TagRenamer {
        fn name(&self) -> &'static str {
            "tag-renamer"
        }
        fn run(&self, root: &crate::dom::engine::NodeRef, _ctx: &DomCtx) {
            // Walk the tree and append a marker comment to body to prove a
            // pass actually executed.
            for node in root.descendants() {
                if let Some(el) = node.as_element() {
                    if &*el.name.local == "p" {
                        let comment = crate::dom::engine::NodeRef::new_comment("touched");
                        node.insert_before(comment);
                        return;
                    }
                }
            }
        }
    }

    #[test]
    fn pass_runs_when_registered() {
        let html = "<html><body><p>hi</p></body></html>";
        let ctx = DomCtx::new(None, false);
        let passes: Vec<Box<dyn DomPass>> = vec![Box::new(TagRenamer)];
        let out = run_passes(html, &ctx, &passes);
        assert!(
            out.contains("<!--touched-->"),
            "pass should have run, got: {out}"
        );
    }
}
