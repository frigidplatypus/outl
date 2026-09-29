//! Pipe tables in the outl dialect.
//!
//! A top-level GFM pipe table is **not** an outline: it has no `- ` marker
//! and no `key:: value`, so the grammar cannot place it. Before this
//! module, each of its lines fell through to the depth-0 recovery arm in
//! [`mod@crate::parse`] and became one verbatim block *per line*, each with an
//! `UnrecognizedBlockMarker` warning — a nine-row table was nine orphan
//! blocks and nine warnings, and the TUI drew a wall of `!` glyphs where a
//! grid should sit.
//!
//! This module owns two things:
//!
//! - **Recognition** (`looks_like_table_start`, `is_table_row`,
//!   `is_delimiter_row`): the predicate the parser uses to decide whether
//!   a run of lines is a table and should collapse.
//! - **Structure** (`Table`, `Align`, [`parse_table_block`]): the column
//!   grid a renderer draws, recovered from a block's stored text.
//!
//! ## One block, not one per line
//!
//! The parser collapses a whole table into a single [`OutlineNode`] whose
//! `text` is the pipe rows joined by `\n` — exactly the shape a fenced
//! code block already uses, and for the same reason: the bytes belong to
//! one block so they round-trip as a unit.
//!
//! The collapse is a **fixpoint** because it is the continuation path
//! doing the round-trip, not a new one. The first parse (a raw import)
//! reads the table at column 0 and stores each row trimmed; the renderer
//! writes the first row after `- ` and the rest at `indent + 1`; every
//! later parse reassembles those indented continuation lines back into the
//! same trimmed, `\n`-joined text. Storage stays byte-verbatim — alignment
//! is derived only at render time (invariant 8: the projection never
//! rewrites what the op log holds).
//!
//! ## Deliberately not a full GFM table parser
//!
//! Recognition is conservative on purpose: it collapses only when the
//! header, delimiter and first data row are all at column 0 and the
//! delimiter row's every cell is `:?-+:?`. A paragraph whose second line
//! happens to contain a `|` is never mistaken for a table, because that
//! line is not a delimiter row. A malformed-but-recognised table (a blank
//! delimiter cell, a ragged row) is accepted and survives as verbatim
//! block text rather than being refused — refusing would split it back
//! into one block per line, which is the loss this module exists to
//! prevent. Cells may be ragged; [`parse_table_block`] pads short rows and
//! truncates long ones to the header's column count.

use crate::parse::OutlineNode;

/// Horizontal alignment of a table column, read from its delimiter cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// No colons, or a leading colon (`:---`).
    Left,
    /// Colons on both sides (`:---:`).
    Center,
    /// A trailing colon (`---:`).
    Right,
}

/// A pipe table recovered from a block's stored text.
///
/// [`Self::header`] and each row in [`Self::rows`] have exactly
/// [`Self::alignments`] entries: short rows are padded with empty cells,
/// long rows truncated to the header's width. A cell has already been
/// unescaped (`\|` → `|`, `\\` → `\`) and trimmed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    /// One alignment per column, taken from the delimiter row.
    pub alignments: Vec<Align>,
    /// The header row's cells.
    pub header: Vec<String>,
    /// The data rows, in source order.
    pub rows: Vec<Vec<String>>,
}

/// Leading spaces of a line (the outline's two-space indent unit; tabs are
/// not used by the renderer, and a tab here simply is not a leading space,
/// so it does not count as indentation and the row is left as content).
fn leading_indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// A row of a pipe table: non-empty once trimmed, and carrying a `|`.
pub(crate) fn is_table_row(line: &str) -> bool {
    let t = line.trim();
    !t.is_empty() && t.contains('|')
}

/// One cell of a delimiter row: optional surrounding spaces and colons
/// around one or more `-`.
fn is_delimiter_cell(cell: &str) -> bool {
    let c = cell.trim();
    let core = c.trim_start_matches(':').trim_end_matches(':');
    !core.is_empty() && core.chars().all(|ch| ch == '-')
}

/// The table's alignment row (`|---|:--:|--:|`).
///
/// A run of one-or-more dashes per cell, optionally fenced with colons,
/// with a `|` somewhere in the line. The check is strict on cells: once a
/// leading and trailing pipe are stripped, every remaining cell must be a
/// delimiter cell, so a header row (`|a|b|`) never reads as a delimiter.
pub(crate) fn is_delimiter_row(line: &str) -> bool {
    let t = line.trim();
    if !t.contains('|') {
        return false;
    }
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    if inner.trim().is_empty() {
        return false;
    }
    inner.split('|').all(is_delimiter_cell)
}

/// Split a header or data row into cells, honouring `\|` as a literal pipe
/// and `\\` as a literal backslash (both unescaped), and trimming each cell.
/// Leading and trailing pipes are the row's fences, not cells.
pub(crate) fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&'\\') => {
                cur.push('\\');
                chars.next();
            }
            '\\' if chars.peek() == Some(&'|') => {
                cur.push('|');
                chars.next();
            }
            '|' => {
                cells.push(cur.trim().to_string());
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

/// Alignment a delimiter cell declares.
fn cell_align(cell: &str) -> Align {
    let c = cell.trim();
    match (c.starts_with(':'), c.ends_with(':')) {
        (true, true) => Align::Center,
        (false, true) => Align::Right,
        _ => Align::Left,
    }
}

/// Whether `lines[start]` opens a top-level table the parser should
/// collapse: a header row, a delimiter row beneath it, and at least one
/// data row after that, all at column 0.
pub(crate) fn looks_like_table_start(lines: &[&str], start: usize) -> bool {
    let Some(header) = lines.get(start) else {
        return false;
    };
    if leading_indent(header) != 0 || !is_table_row(header) || is_delimiter_row(header) {
        return false;
    }
    let Some(delim) = lines.get(start + 1) else {
        return false;
    };
    if leading_indent(delim) != 0 || !is_delimiter_row(delim) {
        return false;
    }
    let Some(first_data) = lines.get(start + 2) else {
        return false;
    };
    leading_indent(first_data) == 0 && is_table_row(first_data) && !is_delimiter_row(first_data)
}

/// Consume a top-level table starting at `*i` into one block's text.
///
/// Advances `*i` past every column-0 row containing a `|` and returns the
/// rows, trimmed and `\n`-joined, or `None` when `*i` does not open a
/// table (leaving `*i` untouched so the caller's normal arm runs). The
/// caller has already gated with [`looks_like_table_start`]; re-checking
/// here keeps the helper safe to call on its own.
pub(crate) fn consume_table_block(lines: &[&str], i: &mut usize) -> Option<String> {
    if !looks_like_table_start(lines, *i) {
        return None;
    }
    let mut end = *i;
    while let Some(line) = lines.get(end) {
        if leading_indent(line) != 0 || !is_table_row(line) {
            break;
        }
        end += 1;
    }
    let text = lines[*i..end]
        .iter()
        .map(|line| line.trim())
        .collect::<Vec<_>>()
        .join("\n");
    *i = end;
    Some(text)
}

/// Recover the column grid from a block's stored `text`.
///
/// Returns `None` unless the text is a whole table: three or more lines
/// whose first is a header row, second a delimiter row, and every line
/// after a data row. This is the [`OutlineNode`] a collapsed table becomes
/// (see the module doc); a block that is not a pure table — prose, a fence,
/// a table glued to a bullet — is `None`, and the caller renders its text
/// as ordinary content.
pub fn parse_table_block(text: &str) -> Option<Table> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 3 {
        return None;
    }
    let header = lines[0];
    if !is_table_row(header) || is_delimiter_row(header) {
        return None;
    }
    if !is_delimiter_row(lines[1]) {
        return None;
    }
    if !lines[2..].iter().all(|line| is_table_row(line)) {
        return None;
    }

    let header_cells = split_row(header);
    let delim_cells = split_row(lines[1]);
    let width = header_cells.len();

    let alignments = (0..width)
        .map(|idx| match delim_cells.get(idx).map(String::as_str) {
            Some(cell) if !cell.is_empty() => cell_align(cell),
            _ => Align::Left,
        })
        .collect();

    let header = header_cells;
    let rows = lines[2..]
        .iter()
        .map(|line| {
            let mut cells = split_row(line);
            cells.resize(width, String::new());
            cells
        })
        .collect();

    Some(Table {
        alignments,
        header,
        rows,
    })
}

/// Whether an [`OutlineNode`]'s text is a whole table.
///
/// Convenience over [`parse_table_block`] for a renderer walking the AST,
/// so it can branch on "grid or prose" without discarding the grid it just
/// parsed. The TUI calls this once per visible block, then hands the
/// [`Table`] to its grid drawer.
pub fn table_from_node(node: &OutlineNode) -> Option<Table> {
    parse_table_block(&node.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(text: &str) -> OutlineNode {
        OutlineNode {
            text: text.to_string(),
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    #[test]
    fn delimiter_row_is_recognised_and_header_is_not() {
        assert!(is_delimiter_row("|---|---|"));
        assert!(is_delimiter_row("| :--- | :---: | ---: |"));
        assert!(is_delimiter_row("---|---"));
        assert!(!is_delimiter_row("|a|b|"), "a header is not a delimiter");
        assert!(!is_delimiter_row("|---|x|"), "one bad cell rejects the row");
        assert!(!is_delimiter_row("a paragraph with | a pipe"));
        assert!(!is_delimiter_row("||"), "no cells");
    }

    #[test]
    fn split_row_unescapes_pipes_and_trims() {
        assert_eq!(split_row("| a | b |"), vec!["a", "b"]);
        assert_eq!(split_row("a | b"), vec!["a", "b"]);
        assert_eq!(
            split_row(r"| x \| y | z |"),
            vec!["x | y", "z"],
            "an escaped pipe is a literal, not a column break"
        );
        assert_eq!(
            split_row(r"| a \\| b |"),
            vec!["a \\", "b"],
            "a literal backslash does not escape the following column separator"
        );
    }

    #[test]
    fn parse_reads_header_delimiter_and_alignment() {
        let t = parse_table_block("| a | b |\n|:--|--:|\n| 1 | 2 |").expect("a table");
        assert_eq!(t.header, vec!["a", "b"]);
        assert_eq!(t.alignments, vec![Align::Left, Align::Right]);
        assert_eq!(t.rows, vec![vec!["1".to_string(), "2".to_string()]]);
    }

    #[test]
    fn parse_pads_short_rows_and_truncates_long_ones() {
        let t = parse_table_block("| a | b | c |\n|---|---|---|\n| 1 |\n| 1 | 2 | 3 | 4 |")
            .expect("a table");
        assert_eq!(t.rows[0], vec!["1", "", ""]);
        assert_eq!(t.rows[1], vec!["1", "2", "3"]);
    }

    #[test]
    fn parse_rejects_anything_that_is_not_a_whole_table() {
        assert!(parse_table_block("just prose").is_none());
        assert!(
            parse_table_block("| a |\n|---|").is_none(),
            "needs a data row"
        );
        assert!(
            parse_table_block("| a |\n|---|\nprose after").is_none(),
            "a trailing prose line means this is not a pure table"
        );
    }

    #[test]
    fn center_alignment_needs_colons_both_sides() {
        let t = parse_table_block("| a |\n|:-:|\n| 1 |").expect("a table");
        assert_eq!(t.alignments, vec![Align::Center]);
    }

    #[test]
    fn table_from_node_rounds_trip_with_the_parser_shape() {
        // The text the parser stores for a collapsed table is exactly what
        // the renderer must recover a grid from.
        let collapsed = node("| a | b |\n|---|---|\n| 1 | 2 |");
        let t = table_from_node(&collapsed).expect("the collapsed block is a table");
        assert_eq!(t.header, vec!["a", "b"]);
        assert!(table_from_node(&node("a bullet's prose")).is_none());
    }

    #[test]
    fn consume_advances_only_over_a_real_table() {
        let table = ["| a | b |", "|---|---|", "| 1 | 2 |", "- next"];
        let mut i = 0;
        let text = consume_table_block(&table, &mut i).expect("a table");
        assert_eq!(text, "| a | b |\n|---|---|\n| 1 | 2 |");
        assert_eq!(i, 3, "stopped before the bullet");

        let mut j = 0;
        assert!(
            consume_table_block(&["free text", "more text"], &mut j).is_none(),
            "prose is not a table"
        );
        assert_eq!(j, 0, "a rejected line is left for the caller");
    }
}
