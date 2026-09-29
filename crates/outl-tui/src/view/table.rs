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

use crate::icons;
use crate::theme::Theme;
use crate::view::outline::FoldMarker;
use outl_md::{Align, Table};

/// Cells painted between columns: a dim vertical bar with one space each
/// side, echoing the `|` the user wrote while reading as a column rule.
const COLUMN_RULE: &str = " │ ";

/// Render `table` as a run of [`Line`]s appended to `out`, prefixed with
/// the same `│ ` indent rails the outline's other rows use.
///
/// When `carries_bullet` the first visual line carries the block's bullet
/// head (fold glyph, optional auto-run bolt, `- `); the rule and every data
/// line carry a blank continuation head so the columns stay flush under the
/// header. When it is `false` the grid sits *inside* a block whose bullet is
/// on some earlier line (a mid-prose table), so its own first line takes a
/// continuation head too and no fold glyph is drawn.
///
/// When `box_style` a **standalone** table (`carries_bullet`) is wrapped in
/// a full frame: a top border row, a `│` wall on each side of every row, and
/// a bottom border row. A mid-prose / nested table is left open regardless,
/// because its side walls would land on the carrying block's indent rails.
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
    carries_bullet: bool,
    table: &Table,
    theme: &Theme,
    out: &mut Vec<Line<'static>>,
    text_width: u16,
    box_style: bool,
) {
    let ncols = column_count(table);
    if ncols == 0 {
        return;
    }

    // Only a standalone table is boxed: a mid-prose / nested run draws
    // under the carrying block's content column, where a `│` wall would
    // collide with its indent rails, so it keeps the open style.
    let boxed = box_style && carries_bullet;

    let mut widths = column_widths(table, ncols);
    // Two extra columns when boxed: the left and right walls. Fold them
    // into the prefix so `fit_widths` shrinks the cells, not the frame.
    let prefix_w = prefix_width(indent, has_auto_run) + usize::from(boxed) * 2;
    let available = if text_width == 0 {
        usize::MAX
    } else {
        (text_width as usize).saturating_sub(prefix_w)
    };
    fit_widths(&mut widths, ncols, available);

    let guides: Vec<Span<'static>> =
        std::iter::repeat_n(Span::styled("│ ", theme.dim), indent as usize).collect();
    let first_head = if carries_bullet {
        bullet_head(bullet_style, has_auto_run, fold, theme)
    } else {
        continuation_head(has_auto_run)
    };
    let cont_head = continuation_head(has_auto_run);

    if boxed {
        out.push(push_row(
            &guides,
            &cont_head,
            border_cells(&widths, "┌", "┬", "┐", theme),
        ));
    }

    let header = table_cells(&table.header, &widths, table, theme, theme.heading);
    out.push(push_row(
        &guides,
        &first_head,
        with_walls(boxed, header, theme),
    ));

    let rule = rule_cells(&widths, table, theme);
    out.push(push_row(
        &guides,
        &cont_head,
        with_walls(boxed, rule, theme),
    ));

    let body = Style::default().fg(theme.foreground);
    for row in &table.rows {
        let cells = table_cells(row, &widths, table, theme, body);
        out.push(push_row(
            &guides,
            &cont_head,
            with_walls(boxed, cells, theme),
        ));
    }

    if boxed {
        out.push(push_row(
            &guides,
            &cont_head,
            border_cells(&widths, "└", "┴", "┘", theme),
        ));
    }
}

/// Wrap a row's cell spans in a `│` wall on each side when `boxed`,
/// returning them untouched for the open style.
fn with_walls(boxed: bool, cells: Vec<Span<'static>>, theme: &Theme) -> Vec<Span<'static>> {
    if !boxed {
        return cells;
    }
    let mut spans = Vec::with_capacity(cells.len() + 2);
    spans.push(Span::styled("│", theme.dim));
    spans.extend(cells);
    spans.push(Span::styled("│", theme.dim));
    spans
}

/// Border / divider row spanning the same columns as the data rows: `left`
/// corner, a `─` run per column, a `mid` tee joining them across the gutter,
/// and a `right` corner. The connector is three cells wide (`─X─`) to match
/// the ` │ ` gutter, so the border aligns glyph-for-glyph with the `│` rules
/// above / below it.
fn border_cells(
    widths: &[usize],
    left: &str,
    mid: &str,
    right: &str,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    spans.push(Span::styled(left.to_string(), theme.dim));
    for (i, &w) in widths.iter().enumerate() {
        spans.push(Span::styled("─".repeat(w), theme.dim));
        if i + 1 < widths.len() {
            spans.push(Span::styled(format!("─{mid}─"), theme.dim));
        }
    }
    spans.push(Span::styled(right.to_string(), theme.dim));
    spans
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
fn prefix_width(indent: u32, has_auto_run: bool) -> usize {
    2 * indent as usize + 4 + usize::from(has_auto_run)
}

/// Bullet head for the table's first line, mirroring the outline's
/// `BlockRowKind::Bullet` head: fold slot, optional auto-run bolt, bullet.
fn bullet_head(
    bullet_style: Style,
    has_auto_run: bool,
    fold: FoldMarker,
    theme: &Theme,
) -> Vec<Span<'static>> {
    let mut head = Vec::new();
    match fold {
        FoldMarker::None => head.push(Span::raw("  ")),
        FoldMarker::Expanded => head.push(Span::styled("▼ ", theme.dim)),
        FoldMarker::Collapsed => head.push(Span::styled("▶ ", theme.hint)),
    }
    if has_auto_run {
        head.push(Span::styled(icons::BOLT, theme.hint));
    }
    head.push(Span::styled("- ", bullet_style));
    head
}

/// Blank head for the rule and data lines, mirroring the outline's
/// `BlockRowKind::Continuation` head so those columns align under the
/// header above them.
fn continuation_head(has_auto_run: bool) -> Vec<Span<'static>> {
    let mut head = vec![Span::raw("  ")];
    if has_auto_run {
        head.push(Span::raw(" "));
    }
    head.push(Span::raw("  "));
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

    fn parse(md: &str) -> Table {
        outl_md::parse_table_block(md).expect("sample must parse as a table")
    }

    const SIMPLE: &str = "| Name | Qty | Price |\n|:---|---:|:---:|\n| Apple | 3 | 1.50 |";

    fn render(table: &Table, width: u16) -> Vec<Line<'static>> {
        render_styled(table, width, false)
    }

    fn render_styled(table: &Table, width: u16, box_style: bool) -> Vec<Line<'static>> {
        let mut out = Vec::new();
        emit_table_lines(
            0,
            theme().bullet,
            false,
            FoldMarker::None,
            true,
            table,
            &theme(),
            &mut out,
            width,
            box_style,
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
            true,
            &parse(SIMPLE),
            &theme(),
            &mut out,
            0,
            false,
        );
        // Bullet line opens with the rail, the fold glyph and a bolt; the
        // continuation line pads the same total width, so both share it.
        assert_eq!(line_width(&out[0]), line_width(&out[1]));
        let bullet = line_text(&out[0]);
        assert!(bullet.starts_with("│ "), "indent rail missing: {bullet:?}");
        assert!(bullet.contains("▶ "), "fold glyph missing: {bullet:?}");
    }

    #[test]
    fn a_boxed_standalone_table_frames_top_and_bottom() {
        let lines = render_styled(&parse(SIMPLE), 0, true);
        // open grid is 3 lines; the box adds a top border and a bottom one.
        assert_eq!(
            lines.len(),
            2 + 3,
            "top border + header + rule + data + bottom"
        );
        let top = line_text(&lines[0]);
        let bottom = line_text(&lines[lines.len() - 1]);
        assert!(
            top.contains('┌') && top.contains('┬') && top.contains('┐'),
            "top: {top:?}"
        );
        assert!(
            bottom.contains('└') && bottom.contains('┴') && bottom.contains('┘'),
            "bottom: {bottom:?}"
        );
        // Header, rule and data rows now carry a side wall on each end.
        for row in &lines[1..lines.len() - 1] {
            let body = line_text(row);
            assert!(body.contains('│'), "row wall missing: {body:?}");
        }
    }

    #[test]
    fn a_boxed_grid_shares_one_display_width_across_every_row() {
        let lines = render_styled(&parse(SIMPLE), 0, true);
        let w = line_width(&lines[0]);
        assert!(
            lines.iter().all(|l| line_width(l) == w),
            "box rows differ: {lines:?}"
        );
    }

    #[test]
    fn the_box_border_aligns_with_the_column_rules() {
        // The `┬` / `┴` tees sit exactly where the interior `│` rules do, so
        // the border is not a separate width — the tee count equals the
        // number of interior gutters (ncols - 1).
        let lines = render_styled(&parse(SIMPLE), 0, true);
        let top = line_text(&lines[0]);
        let tees = top.chars().filter(|c| *c == '┬').count();
        assert_eq!(tees, 2, "three columns → two interior tees: {top:?}");
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
        let app = App::new(
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

    const MID_PROSE: &str = "intro\n| Name | Qty |\n|:--|--:|\n| Apple | 3 |\noutro";

    /// A table sitting *inside* a block renders as a grid wedged between its
    /// prose lines, not one block of raw pipes and not a hijacked paragraph.
    #[test]
    fn a_mid_prose_table_renders_as_a_grid_between_its_prose() {
        let (app, _d) = app();
        let out = via_outline(
            &app,
            RenderMode::Pretty {
                text: MID_PROSE.into(),
            },
        );
        // intro row + header + rule + data + outro row.
        assert_eq!(out.len(), 5, "prose + 3 grid rows + prose: {out:?}");
        assert!(line_text(&out[0]).contains("intro"), "leading prose kept");
        assert!(
            !line_text(&out[0]).contains('|'),
            "leading prose is not raw"
        );
        assert!(line_text(&out[1]).contains("Name"), "grid header");
        assert!(line_text(&out[2]).contains('-'), "grid rule");
        assert!(line_text(&out[3]).contains("Apple"), "grid data");
        assert!(line_text(&out[4]).contains("outro"), "trailing prose kept");
        for line in &out[1..=3] {
            assert!(!line_text(line).contains('|'), "grid has no raw pipes");
        }
    }

    /// The grid drawn for a mid-block run must not re-draw the block's
    /// bullet — the bullet belongs to the block's first line, which here is
    /// the prose. The grid header takes a continuation head instead.
    #[test]
    fn a_mid_prose_grid_carries_no_bullet() {
        let (app, _d) = app();
        let out = via_outline(
            &app,
            RenderMode::Pretty {
                text: MID_PROSE.into(),
            },
        );
        let header_row = line_text(&out[1]);
        assert!(
            !header_row.contains("- "),
            "grid header must not carry a bullet: {header_row:?}"
        );
    }

    /// A mid-block grid nested under a parent draws the indent rails once,
    /// aligned to the parent's content column — never a second set stacked
    /// on top of the block's own rails.
    #[test]
    fn a_nested_mid_prose_grid_draws_the_parents_rails() {
        let (app, _d) = app();
        let mut out = Vec::new();
        emit_block_lines(
            2,
            app.theme.bullet,
            &RenderMode::Pretty {
                text: MID_PROSE.into(),
            },
            false,
            FoldMarker::None,
            &app,
            &mut out,
            0,
        );
        // The grid header (out[1]) opens with exactly two `│ ` rails.
        let header_row = line_text(&out[1]);
        assert!(
            header_row.starts_with("│ │ "),
            "expected two indent rails: {header_row:?}"
        );
        assert!(header_row.contains("Name"), "still the header row");
    }

    /// With `box_tables` on, a standalone table reaches the outline boxed:
    /// two extra border rows around the open grid, corners present.
    #[test]
    fn a_standalone_table_reaches_the_outline_boxed_when_configured() {
        let (mut app, _d) = app();
        app.box_tables = true;
        let out = via_outline(
            &app,
            RenderMode::Pretty {
                text: SIMPLE.into(),
            },
        );
        assert_eq!(out.len(), 2 + 3, "top border + grid + bottom border");
        assert!(line_text(&out[0]).contains('┌'), "top border");
        assert!(
            line_text(&out[out.len() - 1]).contains('└'),
            "bottom border"
        );
    }

    /// `box_tables` never boxes a table sitting inside prose: the side walls
    /// would land on the carrying block's content column, so the open grid
    /// stays regardless of the flag.
    #[test]
    fn a_mid_prose_table_is_never_boxed() {
        let (mut app, _d) = app();
        app.box_tables = true;
        let out = via_outline(
            &app,
            RenderMode::Pretty {
                text: MID_PROSE.into(),
            },
        );
        assert_eq!(out.len(), 5, "still prose + 3 grid rows + prose");
        for line in &out {
            let body = line_text(line);
            assert!(
                !body.contains('┌') && !body.contains('└') && !body.contains('┐'),
                "mid-prose grid must stay open: {body:?}"
            );
        }
    }
}
