//! Runtime-selected icons for TUI chrome.
//!
//! Emoji is the default because it works with ordinary terminal fonts.
//! Nerd Font glyphs are opt-in through `[tui] icons = "nerd-font"`.
//!
//! Scope: every TUI-owned icon — status/footer chips, fold markers,
//! property/command/palette glyphs. What stays Unicode in both sets by
//! design: task checkboxes (`☐`/`◐`/`☑`, they mirror the document
//! state), calendar day dots, scrollbar symbols, and plain geometric
//! separators (`↪`, `≡`, `✕`, `·`).

use crate::theme::Theme;
use crate::view::outline::FoldMarker;
use outl_config::TuiIconStyle;
use ratatui::text::Span;

/// Icons used by the TUI's own chrome and placeholders.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IconSet {
    pub(crate) calendar: &'static str,
    pub(crate) file: &'static str,
    pub(crate) image: &'static str,
    pub(crate) clock: &'static str,
    pub(crate) star: &'static str,
    pub(crate) history: &'static str,
    pub(crate) bolt: &'static str,
    pub(crate) search: &'static str,
    pub(crate) cog: &'static str,
    pub(crate) paint_brush: &'static str,
    pub(crate) warning: &'static str,
    pub(crate) save: &'static str,
    pub(crate) clipboard: &'static str,
    pub(crate) moon: &'static str,
    pub(crate) hashtag: &'static str,
    pub(crate) bell: &'static str,
    pub(crate) play: &'static str,
    /// TODO-progress header chip (`nf-fa-check-square-o`).
    pub(crate) todo_chip: &'static str,
    /// Insert-mode footer chip (`nf-fa-circle`).
    pub(crate) editing: &'static str,
    /// "saved Ns ago" freshness chip (`nf-fa-refresh`).
    pub(crate) freshness: &'static str,
    /// Workspace-name footer chip (`nf-fa-circle-thin`).
    pub(crate) workspace: &'static str,
    /// Backlink-count footer chip (`nf-fa-link`).
    pub(crate) backlinks: &'static str,
    /// Fold marker before an expanded parent; carries its own padding
    /// space so columns stay flush (`nf-fa-chevron-down`).
    pub(crate) fold_open: &'static str,
    /// Fold marker before a collapsed parent (`nf-fa-chevron-right`).
    pub(crate) fold_closed: &'static str,
    /// Help-overlay legend line explaining the two fold markers.
    pub(crate) fold_legend: &'static str,
}

impl IconSet {
    pub(crate) fn new(style: TuiIconStyle) -> Self {
        match style {
            TuiIconStyle::Emoji => Self::emoji(),
            TuiIconStyle::NerdFont => Self::nerd_font(),
        }
    }

    pub(crate) fn property_glyph(&self, key: &str) -> Option<&'static str> {
        match key.to_ascii_lowercase().as_str() {
            outl_md::remind::REMIND_KEY => Some(self.bell),
            "auto-run" => Some(self.play),
            "template" => Some(self.clipboard),
            _ => None,
        }
    }

    pub(crate) fn category_glyph(&self, category: &str) -> &'static str {
        match category {
            "Actions" => self.bolt,
            "Navigation" => "↪",
            "Search" => self.search,
            "Settings" => self.cog,
            "Dates & time" => self.calendar,
            _ => "•",
        }
    }

    /// The fold marker as a styled span, glyph and colour together.
    /// The `None` arm keeps the two-cell gap so leaf bullets stay
    /// aligned with their parent's.
    pub(crate) fn fold_span(&self, marker: FoldMarker, theme: &Theme) -> Span<'static> {
        match marker {
            FoldMarker::None => Span::raw("  "),
            FoldMarker::Expanded => Span::styled(self.fold_open, theme.dim),
            FoldMarker::Collapsed => Span::styled(self.fold_closed, theme.hint),
        }
    }

    pub(crate) fn command_glyph(&self, name: &str) -> &'static str {
        match name {
            "run" => self.play,
            "prop" => "≡",
            "search" | "find" => self.search,
            "theme" => self.paint_brush,
            "open" | "switch" => "↪",
            "quit" | "q" => "✕",
            n if n.starts_with("date") || n == "dt" || n == "dy" || n == "dtm" => self.calendar,
            n if n.starts_with("time") => self.clock,
            n if n.starts_with("iso") => self.hashtag,
            n if n.starts_with("week") => self.calendar,
            "stamp" => self.clock,
            _ => "·",
        }
    }

    fn emoji() -> Self {
        Self {
            calendar: "📅",
            file: "📄",
            image: "🖼",
            clock: "🕐",
            star: "⭐",
            history: "🕘",
            bolt: "⚡",
            search: "🔍",
            cog: "⚙",
            paint_brush: "🎨",
            warning: "⚠",
            save: "💾",
            clipboard: "📋",
            moon: "🌙",
            hashtag: "🔢",
            bell: "⏰",
            play: "▶",
            todo_chip: "☑",
            editing: "●",
            freshness: "⟳",
            workspace: "◌",
            backlinks: "⇇",
            fold_open: "▼ ",
            fold_closed: "▶ ",
            fold_legend: "              (▼ expanded · ▶ collapsed · synced via op log)",
        }
    }

    fn nerd_font() -> Self {
        Self {
            calendar: "\u{f073}",
            file: "\u{f016}",
            image: "\u{f03e}",
            clock: "\u{f017}",
            star: "\u{f005}",
            history: "\u{f1da}",
            bolt: "\u{f0e7}",
            search: "\u{f002}",
            cog: "\u{f013}",
            paint_brush: "\u{f1fc}",
            warning: "\u{f071}",
            save: "\u{f0c7}",
            clipboard: "\u{f0ea}",
            moon: "\u{f186}",
            hashtag: "\u{f292}",
            bell: "\u{f0f3}",
            play: "\u{f04b}",
            todo_chip: "\u{f046}",
            editing: "\u{f111}",
            freshness: "\u{f021}",
            workspace: "\u{f1db}",
            backlinks: "\u{f0c1}",
            fold_open: "\u{f078} ",
            fold_closed: "\u{f054} ",
            fold_legend:
                "              (\u{f078} expanded · \u{f054} collapsed · synced via op log)",
        }
    }
}

impl Default for IconSet {
    fn default() -> Self {
        Self::new(TuiIconStyle::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_is_the_default_and_contains_no_private_use_glyphs() {
        let icons = IconSet::default();
        assert_eq!(icons.calendar, "📅");
        assert!(icons
            .file
            .chars()
            .all(|ch| !(0xE000..=0xF8FF).contains(&(ch as u32))));
    }

    #[test]
    fn nerd_font_is_explicit() {
        let icons = IconSet::new(TuiIconStyle::NerdFont);
        assert_eq!(icons.calendar, "\u{f073}");
        assert!(icons
            .calendar
            .chars()
            .any(|ch| (0xE000..=0xF8FF).contains(&(ch as u32))));
    }

    #[test]
    fn play_routes_through_the_icon_set() {
        let emoji = IconSet::new(TuiIconStyle::Emoji);
        assert_eq!(emoji.property_glyph("auto-run"), Some("▶"));
        assert_eq!(emoji.command_glyph("run"), "▶");

        let nerd = IconSet::new(TuiIconStyle::NerdFont);
        assert_eq!(nerd.property_glyph("auto-run"), Some("\u{f04b}"));
        assert_eq!(nerd.command_glyph("run"), "\u{f04b}");
    }

    #[test]
    fn emoji_preserves_the_pre_iconset_glyphs() {
        let emoji = IconSet::new(TuiIconStyle::Emoji);
        assert_eq!(emoji.command_glyph("iso-date-today"), "🔢");
        assert_eq!(emoji.todo_chip, "☑");
        assert_eq!(emoji.fold_open, "▼ ");
        assert_eq!(emoji.fold_closed, "▶ ");
    }

    #[test]
    fn chrome_and_fold_glyphs_route_through_the_set() {
        let nerd = IconSet::new(TuiIconStyle::NerdFont);
        for glyph in [
            nerd.todo_chip,
            nerd.editing,
            nerd.freshness,
            nerd.workspace,
            nerd.backlinks,
            nerd.fold_open,
            nerd.fold_closed,
        ] {
            assert!(
                glyph
                    .chars()
                    .all(|ch| ch == ' ' || (0xE000..=0xF8FF).contains(&(ch as u32))),
                "nerd chip must be PUA-only: {glyph:?}"
            );
        }
        assert!(nerd.fold_legend.contains('\u{f078}'));
        assert!(nerd.fold_legend.contains('\u{f054}'));
    }
}
