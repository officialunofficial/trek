//! Strip layout tables and remove empty tables.
//!
//! A "layout table" is one whose only purpose is positioning — typically
//! a single-cell table or one whose every cell holds a single block element.

use kuchikiki::NodeRef;

use crate::dom::walk::{element_children, is_any_tag, is_visually_empty, unwrap};
use crate::dom::{DomCtx, DomPass};

pub struct Tables;

fn is_layout_table(table: &NodeRef) -> bool {
    // Strip when every <td>/<th> contains at most one child block element.
    let cells: Vec<NodeRef> = table
        .descendants()
        .filter(|d| is_any_tag(d, &["td", "th"]))
        .collect();
    if cells.is_empty() {
        return false;
    }
    // Single-cell layout table.
    if cells.len() == 1 {
        return true;
    }
    // Heuristic 1: only one row and no header cells.
    let rows: Vec<NodeRef> = table
        .descendants()
        .filter(|d| is_any_tag(d, &["tr"]))
        .collect();
    if rows.len() == 1 {
        let has_th = cells.iter().any(|c| is_any_tag(c, &["th"]));
        if !has_th {
            return true;
        }
    }
    false
}

impl DomPass for Tables {
    fn name(&self) -> &'static str {
        "tables"
    }

    fn run(&self, root: &NodeRef, _ctx: &DomCtx) {
        let tables: Vec<NodeRef> = root
            .descendants()
            .filter(|d| is_any_tag(d, &["table"]))
            .collect();
        for t in tables {
            if t.parent().is_none() {
                continue;
            }
            // Empty table → remove entirely.
            if is_visually_empty(&t) {
                t.detach();
                continue;
            }
            // Layout table → unwrap cells (move their content out, drop table).
            if is_layout_table(&t) {
                // Move each cell's children out as siblings of the table.
                let mut nodes_to_move: Vec<NodeRef> = Vec::new();
                for cell in t.descendants() {
                    if is_any_tag(&cell, &["td", "th"]) {
                        for c in cell.children() {
                            nodes_to_move.push(c.clone());
                        }
                    }
                }
                for c in nodes_to_move {
                    t.insert_before(c);
                }
                t.detach();
            }
        }

        // Drop empty tbody/thead/tfoot wrappers (cleanup after cell removals).
        for d in root.descendants().collect::<Vec<_>>() {
            if is_any_tag(&d, &["tbody", "thead", "tfoot"]) {
                let kids = element_children(&d);
                if kids.is_empty() {
                    d.detach();
                } else if kids.len() == 1 && is_any_tag(&kids[0], &["table"]) {
                    // Spurious wrapper — unwrap.
                    unwrap(&d);
                }
            }
        }
    }
}
