//! Row chrome — everything a block draws around its own text.
//!
//! The fold slot, the `auto-run::` marker, the pad every non-bullet row
//! spends to reach the text column, and the `key:: value` rows. One
//! module because they are one measurement: change the fold slot and
//! the property row has to move with it, which is exactly the drift
//! [issue 319](https://github.com/outlmd/outl/issues/319) was.

use crate::icons::IconSet;
use crate::state::App;
use crate::view::wrap::push_wrapped;
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthStr;

/// Blank cells standing in for the `auto-run::` marker on the rows
/// that don't draw it. Measured from the active [`IconSet`] rather
/// than written as a literal: `⚡` is two cells wide and the Nerd Font
/// glyph is one, so a fixed pad put every continuation row of an
/// `auto-run::` block a column off in one of the two modes.
/// `the_auto_run_pad_matches_the_glyph` keeps the pair honest.
fn auto_run_pad(icons: &IconSet) -> String {
    " ".repeat(icons.bolt.width())
}

/// Pad the cells a bullet row spends between the indent guides and
/// the block's text: the two-cell fold slot, the optional `auto-run::`
/// marker, and the `- ` bullet.
///
/// Every other row a block emits pads by exactly this, so all of them
/// start in the block's own text column (#319).
pub(crate) fn push_body_indent(
    spans: &mut Vec<Span<'static>>,
    has_auto_run: bool,
    icons: &IconSet,
) {
    spans.push(Span::raw("    "));
    if has_auto_run {
        spans.push(Span::raw(auto_run_pad(icons)));
    }
}

/// Emit one `key:: value` row under the block that carries it.
///
/// Single owner for the property row, because there are two callers
/// (the outline and the backlinks mini-outline) and they had already
/// drifted: backlinks never drew the [`property_glyph`], so the same
/// `remind::` read differently depending on which pane you saw it in.
///
/// The row wraps like any block row — a long `template::` used to run
/// off the right edge and get clipped with nothing to say it had been.
/// The glyph rides in the `head`, so a wrapped value re-indents under
/// the key rather than under the glyph.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_property_row(
    indent: u32,
    key: &str,
    value: &str,
    has_auto_run: bool,
    app: &App,
    out: &mut Vec<Line<'static>>,
    text_width: u16,
) {
    let mut guides: Vec<Span<'static>> = Vec::new();
    for _ in 0..indent {
        guides.push(Span::styled("│ ", app.theme.dim));
    }
    let mut head: Vec<Span<'static>> = Vec::new();
    push_body_indent(&mut head, has_auto_run, &app.icons);
    if let Some(glyph) = property_glyph(key, &app.icons) {
        head.push(Span::raw(format!("{glyph} ")));
    }
    let content = vec![
        Span::styled(format!("{key}:: "), app.theme.property_key),
        Span::styled(value.to_string(), app.theme.property_value),
    ];
    push_wrapped(guides, head, content, text_width, None, out);
}

/// Fold indicator drawn before the bullet on the bullet row.
///
/// `None` keeps a two-cell gap so leaf rows align with their parent
/// at the same indent — without it, a leaf's `-` would slide left
/// the moment a sibling grew children.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FoldMarker {
    /// Block has no children — no marker, gap only.
    None,
    /// Block has children and they're visible. `▼ ` prefix.
    Expanded,
    /// Block has children but they're folded away. `▶ ` prefix.
    Collapsed,
}

/// Leading glyph for a property key outl gives a meaning to.
///
/// Mirrors `KNOWN_PROPERTIES` in
/// `@outl/shared/markdown/properties` — a const can't cross the
/// Rust/TS boundary any more than a DTO field can, so the two tables
/// are edited together. A user's own key (`priority::`) gets no glyph;
/// interpreting it isn't ours to do.
pub(crate) fn property_glyph(key: &str, icons: &IconSet) -> Option<&'static str> {
    match key.to_ascii_lowercase().as_str() {
        outl_md::remind::REMIND_KEY => Some(icons.bell),
        "auto-run" => Some("▶"),
        "template" => Some(icons.clipboard),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use outl_config::TuiIconStyle;

    /// The blank stand-in has to measure what the glyph measures, or
    /// every row that doesn't draw the marker sits a column off the
    /// one that does. That was the bug, for as long as the pad was a
    /// literal, and it comes back the moment one icon style's glyph
    /// stops matching the other's width.
    #[test]
    fn the_auto_run_pad_matches_the_glyph() {
        for style in [TuiIconStyle::Emoji, TuiIconStyle::NerdFont] {
            let icons = IconSet::new(style);
            assert_eq!(
                auto_run_pad(&icons).width(),
                icons.bolt.width(),
                "pad drifted from the glyph in {style:?} mode"
            );
        }
    }
}
