//! Pipe-table grid rendering for the outline.
//!
//! A block that `outl_md` collapsed into one pipe table (RFC 0329) would
//! otherwise render as N raw `| … |` continuation rows. This module turns
//! such a block into an aligned grid: a header row, a rule showing each
//! column's declared alignment, and one row per data line — the columns
//! padded to a common width so the table reads as a table.
//!
//! The grid only ever appears in *pretty* render. A selected or edited
//! table block keeps its raw source (see the caller in `outline.rs`), so
//! the cursor's columns stay byte-aligned with what the user typed.
//!
//! Cells are plain text here: a table cell is not run through the inline
//! markdown tokeniser (refs, bold, links stay literal), which is the
//! documented v1 scope. Column widths are measured with `unicode_width`
//! so wide glyphs (CJK, emoji) occupy the two cells they actually paint.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;
use unicode_width::UnicodeWidthStr;

use crate::icons::IconSet;
use crate::theme::Theme;
use crate::view::row_chrome::{push_body_indent, FoldMarker};
use outl_md::{Align, Table};

/// Cells painted between columns: a dim vertical bar with one space each
/// side, echoing the `|` the user wrote while reading as a column rule.
const COLUMN_RULE: &str = " │ ";

/// Render `table` as a run of [`Line`]s appended to `out`, prefixed with
/// the same `│ ` indent rails and `- ` bullet head the outline's other
/// rows use. The first visual line carries the bullet head; the rule and
/// every data line carry a blank continuation head so the columns stay
/// flush under the header.
///
/// `text_width == 0` is the "don't size to the pane" sentinel (headless
/// renders) — columns keep their natural width. Otherwise the widest
/// columns are trimmed to fit so the grid never runs off the right edge.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_table_lines(
    indent: u32,
    bullet_style: Style,
    has_auto_run: bool,
    fold: FoldMarker,
    table: &Table,
    theme: &Theme,
    icons: &IconSet,
    out: &mut Vec<Line<'static>>,
    text_width: u16,
) {
    let ncols = column_count(table);
    if ncols == 0 {
        return;
    }

    let mut widths = column_widths(table, ncols);
    let prefix_w = prefix_width(indent, has_auto_run, icons);
    let available = if text_width == 0 {
        usize::MAX
    } else {
        (text_width as usize).saturating_sub(prefix_w)
    };
    fit_widths(&mut widths, ncols, available);

    let guides: Vec<Span<'static>> =
        std::iter::repeat_n(Span::styled("│ ", theme.dim), indent as usize).collect();
    let bullet_head = bullet_head(bullet_style, has_auto_run, fold, theme, icons);
    let cont_head = continuation_head(has_auto_run, icons);

    let header = table_cells(&table.header, &widths, table, theme, theme.heading);
    out.push(push_row(&guides, &bullet_head, header));

    let rule = rule_cells(&widths, table, theme);
    out.push(push_row(&guides, &cont_head, rule));

    let body = Style::default().fg(theme.foreground);
    for row in &table.rows {
        let cells = table_cells(row, &widths, table, theme, body);
        out.push(push_row(&guides, &cont_head, cells));
    }
}

/// Column count: the header's width, but never fewer than the widest data
/// row (a short row is padded with blanks below, a long row still draws).
fn column_count(table: &Table) -> usize {
    table
        .rows
        .iter()
        .map(|r| r.len())
        .max()
        .unwrap_or(0)
        .max(table.header.len())
}

/// Natural (untruncated) display width of every column: the widest of the
/// header and each data cell.
fn column_widths(table: &Table, ncols: usize) -> Vec<usize> {
    (0..ncols)
        .map(|i| {
            let mut w = cell_of(&table.header, i).width();
            for row in &table.rows {
                w = w.max(cell_of(row, i).width());
            }
            w
        })
        .collect()
}

/// Shrink columns in place so `sum(widths) + gutters` fits `available`.
/// Each column is capped to an equal share; a pane too narrow to give
/// every column a single cell is left overflowing rather than dropped —
/// the terminal clips it, matching the outline's "overflow beats loop"
/// rule for pathologically narrow panes.
fn fit_widths(widths: &mut [usize], ncols: usize, available: usize) {
    let gutter = COLUMN_RULE.len() * ncols.saturating_sub(1);
    let cell_budget = available.saturating_sub(gutter);
    let total: usize = widths.iter().sum();
    if total <= cell_budget || cell_budget < ncols {
        return;
    }
    let cap = (cell_budget / ncols).max(1);
    for w in widths.iter_mut() {
        *w = (*w).min(cap);
    }
}

/// Total display width consumed before the table's own content: the
/// indent rails plus the fold/bolt/bullet head (a row's bullet head and
/// continuation head are deliberately the same width).
fn prefix_width(indent: u32, has_auto_run: bool, icons: &IconSet) -> usize {
    let bolt = if has_auto_run { icons.bolt.width() } else { 0 };
    2 * indent as usize + 4 + bolt
}

/// Bullet head for the table's first line, mirroring the outline's
/// `BlockRowKind::Bullet` head: fold slot, optional auto-run bolt, bullet.
fn bullet_head(
    bullet_style: Style,
    has_auto_run: bool,
    fold: FoldMarker,
    theme: &Theme,
    icons: &IconSet,
) -> Vec<Span<'static>> {
    let mut head = vec![icons.fold_span(fold, theme)];
    if has_auto_run {
        head.push(Span::styled(icons.bolt, theme.hint));
    }
    head.push(Span::styled("- ", bullet_style));
    head
}

/// Blank head for the rule and data lines, mirroring the outline's
/// `BlockRowKind::Continuation` head so those columns align under the
/// header above them.
fn continuation_head(has_auto_run: bool, icons: &IconSet) -> Vec<Span<'static>> {
    let mut head = Vec::new();
    push_body_indent(&mut head, has_auto_run, icons);
    head
}

/// Concatenate the rails, a head and the cell spans into one line.
fn push_row(
    guides: &[Span<'static>],
    head: &[Span<'static>],
    cells: Vec<Span<'static>>,
) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = guides.to_vec();
    spans.extend(head.iter().cloned());
    spans.extend(cells);
    Line::from(spans)
}

/// Build the styled, padded, column-ruled spans for one row's cells.
fn table_cells(
    row: &[String],
    widths: &[usize],
    table: &Table,
    theme: &Theme,
    style: Style,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, &w) in widths.iter().enumerate() {
        let padded = pad(truncate(cell_of(row, i), w), w, align_of(table, i));
        spans.push(Span::styled(padded, style));
        if i + 1 < widths.len() {
            spans.push(Span::styled(COLUMN_RULE, theme.dim));
        }
    }
    spans
}

/// Build the alignment rule row: dashes the width of each column, with a
/// leading / trailing `:` (or both) marking left / right / center as the
/// delimiter row declared.
fn rule_cells(widths: &[usize], table: &Table, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (i, &w) in widths.iter().enumerate() {
        spans.push(Span::styled(rule_cell(w, align_of(table, i)), theme.dim));
        if i + 1 < widths.len() {
            spans.push(Span::styled(COLUMN_RULE, theme.dim));
        }
    }
    spans
}

/// The `─`-style rule body for a column of width `w` with `align`.
fn rule_cell(w: usize, align: Align) -> String {
    match align {
        Align::Left if w >= 1 => format!(":{}", "-".repeat(w - 1)),
        Align::Right if w >= 1 => format!("{}:", "-".repeat(w - 1)),
        Align::Center if w >= 2 => format!(":{}:", "-".repeat(w - 2)),
        _ => "-".repeat(w),
    }
}

/// Pad `s` on the side `align` calls for so it occupies exactly `w` cells.
/// `s` is assumed already truncated to at most `w` display cells.
fn pad(s: String, w: usize, align: Align) -> String {
    let gap = w.saturating_sub(s.width());
    match align {
        Align::Left => s + &" ".repeat(gap),
        Align::Right => " ".repeat(gap) + &s,
        Align::Center => {
            let left = gap / 2;
            " ".repeat(left) + &s + &" ".repeat(gap - left)
        }
    }
}

/// Clip `s` to at most `max` display cells, replacing the cut tail with a
/// single `…` when anything was dropped.
fn truncate(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let budget = max - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > budget {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    out
}

/// Cell `i` of `row`, or empty when the row is ragged and stops short.
fn cell_of(row: &[String], i: usize) -> &str {
    row.get(i).map(String::as_str).unwrap_or("")
}

/// Declared alignment for column `i`, defaulting to left when the
/// delimiter row was narrower than the data (defensive; the parser keeps
/// them in step).
fn align_of(table: &Table, i: usize) -> Align {
    table.alignments.get(i).copied().unwrap_or(Align::Left)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn line_width(line: &Line<'_>) -> usize {
        line.spans.iter().map(|s| s.content.width()).sum()
    }

    fn theme() -> Theme {
        crate::theme::default_theme()
    }

    fn icons() -> IconSet {
        IconSet::new(outl_config::TuiIconStyle::Emoji)
    }

    fn parse(md: &str) -> Table {
        outl_md::parse_table_block(md).expect("sample must parse as a table")
    }

    const SIMPLE: &str = "| Name | Qty | Price |\n|:---|---:|:---:|\n| Apple | 3 | 1.50 |";

    fn render(table: &Table, width: u16) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        emit_table_lines(
            0,
            theme().bullet,
            false,
            FoldMarker::None,
            table,
            &theme(),
            &icons(),
            &mut out,
            width,
        );
        out
    }

    #[test]
    fn header_rule_and_rows_each_get_a_line() {
        let lines = render(&parse(SIMPLE), 0);
        assert_eq!(lines.len(), 1 + 1 + 1, "header + rule + one data row");
        assert!(line_text(&lines[0]).contains("Name"));
        assert!(line_text(&lines[1]).contains('-'));
        assert!(line_text(&lines[2]).contains("Apple"));
    }

    #[test]
    fn every_row_shares_one_display_width() {
        let lines = render(&parse(SIMPLE), 0);
        let w = line_width(&lines[0]);
        assert!(
            lines.iter().all(|l| line_width(l) == w),
            "ragged grid: {lines:?}"
        );
    }

    #[test]
    fn right_aligned_column_pads_on_the_left() {
        // The `Qty` column is `---:`; its cell "3" sits at the right of a
        // 5-wide column ("Price"), so the drawn data cell is left-padded.
        let data = &render(&parse(SIMPLE), 0)[2];
        let body = line_text(data);
        // A right-aligned single-char cell is preceded by padding within
        // its column: the vertical rule is not flush against "3".
        assert!(
            body.contains(" 3 ") || body.contains("│ 3 "),
            "right pad: {body:?}"
        );
    }

    #[test]
    fn rule_marks_declared_alignment() {
        let rule = line_text(&render(&parse(SIMPLE), 0)[1]);
        assert!(rule.contains(':'), "alignment colon missing: {rule:?}");
    }

    #[test]
    fn headless_width_keeps_natural_columns() {
        let md = "| a | b |\n| --- | --- |\n| a very long cell | x |";
        let lines = render(&parse(md), 0);
        assert!(line_text(&lines[2]).contains("a very long cell"));
    }

    #[test]
    fn narrow_pane_truncates_without_overrunning() {
        let md = "| a really long first column | another very long column |\n| --- | --- |\n| alpha | beta |";
        let width = 30;
        let lines = render(&parse(md), width);
        for line in &lines {
            assert!(
                line_width(line) <= width as usize,
                "row overran {width}: {}",
                line_width(line)
            );
        }
        // The oversized cells here are the header's, so that is the row
        // carrying the elision; the short data row is untouched.
        assert!(
            line_text(&lines[0]).contains('…'),
            "expected a truncation marker"
        );
        assert!(
            !line_text(&lines[2]).contains('…'),
            "short cells must not elide"
        );
    }

    #[test]
    fn indent_rails_and_bullet_align_all_rows() {
        let mut out = Vec::new();
        emit_table_lines(
            1,
            theme().bullet,
            true,
            FoldMarker::Collapsed,
            &parse(SIMPLE),
            &theme(),
            &icons(),
            &mut out,
            0,
        );
        // Bullet line opens with the rail, the fold glyph and a bolt; the
        // continuation line pads the same total width, so both share it.
        assert_eq!(line_width(&out[0]), line_width(&out[1]));
        let bullet = line_text(&out[0]);
        assert!(bullet.starts_with("│ "), "indent rail missing: {bullet:?}");
        assert!(bullet.contains("▶ "), "fold glyph missing: {bullet:?}");
    }

    // The grid only reaches the outline through `emit_block_lines`; these
    // pin that seam — and that a cursor row is left as raw source.

    use crate::state::App;
    use crate::view::outline::{emit_block_lines, RenderMode};
    use outl_core::id::ActorId;
    use outl_core::workspace::Workspace;
    use tempfile::TempDir;

    fn app() -> (App, TempDir) {
        let dir = TempDir::new().unwrap();
        let actor = ActorId::new();
        let ws = Workspace::open_in_memory(actor).unwrap();
        let app = App::new_for_tests(
            dir.path().to_path_buf(),
            ws,
            actor,
            crate::theme::default_theme(),
            false,
        )
        .unwrap();
        (app, dir)
    }

    fn via_outline(app: &App, mode: RenderMode) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        emit_block_lines(
            0,
            app.theme.bullet,
            &mode,
            false,
            FoldMarker::None,
            app,
            &mut out,
            0,
        );
        out
    }

    #[test]
    fn a_pretty_block_that_is_a_table_renders_as_a_grid() {
        let (app, _d) = app();
        let out = via_outline(
            &app,
            RenderMode::Pretty {
                text: SIMPLE.into(),
            },
        );
        assert_eq!(out.len(), 3, "header + rule + data, not raw lines");
        assert!(line_text(&out[1]).contains('-'), "rule row expected");
        assert!(
            !line_text(&out[2]).contains('|'),
            "no raw pipes in the grid"
        );
    }

    #[test]
    fn an_edited_table_block_keeps_its_raw_source() {
        let (app, _d) = app();
        let out = via_outline(
            &app,
            RenderMode::Editing {
                text: SIMPLE.into(),
                cursor_char: 0,
            },
        );
        assert!(
            out.iter().any(|l| line_text(l).contains('|')),
            "raw pipes kept under cursor"
        );
    }
}
