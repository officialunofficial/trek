//! A vendored, minimal HTML tree engine that replaces the old `kuchikiki`
//! dependency.
//!
//! # Status
//!
//! Every DOM call site in this crate imports from here now, through the
//! single [`crate::dom::parse_html`] entry point. `#[allow(dead_code)]`
//! stays on the whole tree because kuchikiki's own surface (see "Scope"
//! below) is bigger than what Trek calls, and the unused parts stay
//! byte-faithful to the vendored source rather than getting trimmed.
//!
//! # Attribution
//!
//! This module is adapted from [`kuchikiki`], the actively-maintained fork
//! of Simon Sapin's `kuchiki`, itself further forked by Brave as
//! `kuchikiki-speedreader` (crates.io: `kuchikiki`, version
//! `0.8.9-speedreader`, <https://github.com/brave/kuchikiki>).
//!
//! `kuchikiki-speedreader` is MIT licensed:
//!
//! ```text
//! Copyright (c) The kuchiki, kuchikiki, and kuchikiki-speedreader authors,
//! including Simon Sapin, Brave Authors, and Ralph Giles <rgiles@brave.com>.
//!
//! Permission is hereby granted, free of charge, to any person obtaining a
//! copy of this software and associated documentation files (the
//! "Software"), to deal in the Software without restriction, including
//! without limitation the rights to use, copy, modify, merge, publish,
//! distribute, sublicense, and/or sell copies of the Software, and to
//! permit persons to whom the Software is furnished to do so, subject to
//! the following conditions:
//!
//! The above copyright notice and this permission notice shall be included
//! in all copies or substantial portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
//! OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
//! MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
//! IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
//! CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
//! TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
//! SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//! ```
//!
//! Trek's own `Cargo.toml` also declares `license = "MIT"`, so this is
//! license-compatible.
//!
//! # Scope
//!
//! `kuchikiki`'s own surface is bigger than what this crate touches (it
//! also has fragment parsing, extra `NodeData` accessors, and iterator
//! combinators Trek never calls). This module keeps only what Trek's
//! source actually uses: one HTML-document parser entry point
//! ([`parse_html`]), a [`NodeRef`] tree with its navigation and mutation
//! methods, a `selectors`-backed `.select()` / `.select_first()`, and the
//! construction helpers `NodeRef::new_element` / `NodeRef::new_comment`
//! plus the `Attribute` / `ExpandedName` types they take.
//!
//! [`kuchikiki`]: https://crates.io/crates/kuchikiki

// This module is kept byte-faithful to the vendored source below, apart
// from `use` paths (see "Status" above) — so its lints are silenced here
// rather than by reworking known-correct upstream code.
#![allow(dead_code)]
#![allow(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(missing_docs)]

mod attributes;
mod cell_extras;
mod iter;
mod node_data_ref;
mod parser;
mod select;
mod serializer;
mod tree;

pub use attributes::{Attribute, Attributes, ExpandedName};
pub use node_data_ref::NodeDataRef;
pub use parser::{ParseOpts, Sink, parse_html, parse_html_with_options};
pub use select::{Selector, Selectors, Specificity};
pub use tree::{Doctype, DocumentData, ElementData, Node, NodeData, NodeRef};

/// Re-exports of traits that are useful when using this module, mirroring
/// `kuchikiki::traits`.
///
/// ```rust,ignore
/// use crate::dom::engine::traits::*;
/// ```
pub mod traits {
    pub use super::iter::{ElementIterator, NodeIterator};
    pub use html5ever_engine::tendril::TendrilSink;
}
