//! Table → Markdown rendering.

use kuchikiki::NodeRef;

use super::escape::escape_table_cell;
use super::util::{is_any_tag, is_tag};

/// Decide how to render a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    /// Layout table — treat as a transparent wrapper. Render every cell as if
    /// it were ordinary flow content.
    Layout,
    /// Table contains `colspan` / `rowspan` — emit raw HTML (Markdown allows
    /// it). Defuddle does the same.
    Complex,
    /// Standard data table — emit GFM pipe syntax.
    Simple,
    /// Empty — drop entirely.
    Empty,
}

pub fn classify(table: &NodeRef) -> TableKind {
    // Gather all data rows.
    let rows: Vec<NodeRef> = table.descendants().filter(|n| is_tag(n, "tr")).collect();
    if rows.is_empty() {
        return TableKind::Empty;
    }

    let mut has_complex = false;
    let mut max_cells = 0usize;
    let mut nested = false;
    for row in &rows {
        let cells: Vec<NodeRef> = row
            .children()
            .filter(|c| is_any_tag(c, &["td", "th"]))
            .collect();
        max_cells = max_cells.max(cells.len());
        for cell in &cells {
            if let Some(el) = cell.as_element() {
                let attrs = el.attributes.borrow();
                if let Some(cs) = attrs.get("colspan") {
                    if cs.parse::<u32>().is_ok_and(|n| n > 1) {
                        has_complex = true;
                    }
                }
                if let Some(rs) = attrs.get("rowspan") {
                    if rs.parse::<u32>().is_ok_and(|n| n > 1) {
                        has_complex = true;
                    }
                }
            }
            // Nested tables → if present in any cell, this is complex.
            if cell.descendants().any(|d| is_tag(&d, "table")) {
                nested = true;
            }
        }
    }

    if has_complex {
        return TableKind::Complex;
    }
    if !nested && max_cells <= 1 {
        return TableKind::Layout;
    }
    // Empty table: every cell's text content is whitespace.
    let all_empty = rows.iter().all(|r| {
        r.children()
            .filter(|c| is_any_tag(c, &["td", "th"]))
            .all(|c| c.text_contents().trim().is_empty())
    });
    if all_empty {
        return TableKind::Empty;
    }
    TableKind::Simple
}

/// Render a simple table as a GFM pipe table. `cell_render` converts a cell's
/// children to inline markdown text.
pub fn render_simple<F>(table: &NodeRef, mut cell_render: F) -> String
where
    F: FnMut(&NodeRef) -> String,
{
    let rows: Vec<NodeRef> = table.descendants().filter(|n| is_tag(n, "tr")).collect();
    if rows.is_empty() {
        return String::new();
    }

    // Determine header row. If the table has any <th>, use the first row that
    // contains them. Otherwise, treat the first row as header.
    let header_idx = rows
        .iter()
        .position(|r| r.children().any(|c| is_tag(&c, "th")))
        .unwrap_or(0);

    let mut data: Vec<Vec<String>> = Vec::new();
    for row in &rows {
        let cells: Vec<String> = row
            .children()
            .filter(|c| is_any_tag(c, &["td", "th"]))
            .map(|c| escape_table_cell(&cell_render(&c)))
            .collect();
        if !cells.is_empty() {
            data.push(cells);
        }
    }
    if data.is_empty() {
        return String::new();
    }
    let cols = data.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return String::new();
    }

    // Reorder so the header is first.
    if header_idx > 0 && header_idx < data.len() {
        let h = data.remove(header_idx);
        data.insert(0, h);
    }

    let mut out = String::new();
    let header = &data[0];
    out.push('|');
    for i in 0..cols {
        out.push(' ');
        out.push_str(header.get(i).map_or("", String::as_str));
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in 0..cols {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in &data[1..] {
        out.push('|');
        for i in 0..cols {
            out.push(' ');
            out.push_str(row.get(i).map_or("", String::as_str));
            out.push_str(" |");
        }
        out.push('\n');
    }
    out
}
