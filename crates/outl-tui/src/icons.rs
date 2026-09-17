//! Runtime-selected icons for TUI chrome.
//!
//! Emoji is the default because it works with ordinary terminal fonts.
//! Nerd Font glyphs are opt-in through `[tui] icons = "nerd-font"`.

use outl_config::TuiIconStyle;

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
            "auto-run" => Some("▶"),
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

    pub(crate) fn command_glyph(&self, name: &str) -> &'static str {
        match name {
            "run" => "▶",
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
            hashtag: "#",
            bell: "⏰",
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
}
