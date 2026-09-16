//! Icon sets for TUI chrome.
//!
//! The default icon set uses Font Awesome glyphs from the set embedded in any
//! [Nerd Font](https://www.nerdfonts.com) build (`nf-fa-*`). The TUI
//! assumes the terminal runs a Nerd Font; on a font without the PUA
//! cells these render as tofu. That is the deliberate trade for not
//! shipping emoji in the UI.
//!
//! Codepoints verified against the Nerd Fonts 3.5.1 `glyphnames.json`
//! (`fa-*` entries), which embeds Font Awesome 4 at its original
//! codepoints.

use outl_config::TuiIconStyle;

/// Runtime-selected TUI chrome icons.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IconSet {
    pub(crate) calendar: &'static str,
    pub(crate) file: &'static str,
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
    pub(crate) headers: [&'static str; 6],
}

impl IconSet {
    pub(crate) fn new(style: TuiIconStyle) -> Self {
        match style {
            TuiIconStyle::NerdFont => Self::nerd_font(),
            TuiIconStyle::Emoji => Self::emoji(),
        }
    }

    pub(crate) fn header(&self, level: usize) -> &'static str {
        self.headers[level]
    }

    fn nerd_font() -> Self {
        Self {
            calendar: CALENDAR,
            file: FILE,
            clock: CLOCK,
            star: STAR,
            history: HISTORY,
            bolt: BOLT,
            search: SEARCH,
            cog: COG,
            paint_brush: PAINT_BRUSH,
            warning: WARNING,
            save: SAVE,
            clipboard: CLIPBOARD,
            moon: MOON,
            hashtag: HASHTAG,
            bell: BELL,
            headers: [HEADER_1, HEADER_2, HEADER_3, HEADER_4, HEADER_5, HEADER_6],
        }
    }

    fn emoji() -> Self {
        Self {
            calendar: "📅",
            file: "📄",
            clock: "🕒",
            star: "⭐",
            history: "🕘",
            bolt: "⚡",
            search: "🔍",
            cog: "⚙️",
            paint_brush: "🎨",
            warning: "⚠️",
            save: "💾",
            clipboard: "📋",
            moon: "🌙",
            hashtag: "#",
            bell: "🔔",
            headers: ["H1", "H2", "H3", "H4", "H5", "H6"],
        }
    }
}

impl Default for IconSet {
    fn default() -> Self {
        Self::new(TuiIconStyle::NerdFont)
    }
}

/// Journal / calendar (`nf-fa-calendar`).
pub const CALENDAR: &str = "\u{f073}";

/// Generic page / file (`nf-fa-file-o`).
pub const FILE: &str = "\u{f016}";

/// Clock / time (`nf-fa-clock-o`).
pub const CLOCK: &str = "\u{f017}";

/// Pinned / star (`nf-fa-star`).
pub const STAR: &str = "\u{f005}";

/// Recent / history (`nf-fa-history`).
pub const HISTORY: &str = "\u{f1da}";

/// Auto-run / actions (`nf-fa-bolt`).
pub const BOLT: &str = "\u{f0e7}";

/// Search (`nf-fa-search`).
pub const SEARCH: &str = "\u{f002}";

/// Settings (`nf-fa-cog`).
pub const COG: &str = "\u{f013}";

/// Theme / paint (`nf-fa-paint-brush`).
pub const PAINT_BRUSH: &str = "\u{f1fc}";

/// Warning (`nf-fa-exclamation-triangle`).
pub const WARNING: &str = "\u{f071}";

/// Save (`nf-fa-save`).
pub const SAVE: &str = "\u{f0c7}";

/// Image asset placeholder (`nf-fa-image`).
pub const IMAGE: &str = "\u{f03e}";

/// Template / clipboard (`nf-fa-clipboard`).
pub const CLIPBOARD: &str = "\u{f0ea}";

/// Snoozed / moon (`nf-fa-moon-o`).
pub const MOON: &str = "\u{f186}";

/// ISO / number (`nf-fa-hashtag`).
pub const HASHTAG: &str = "\u{f292}";

/// Reminder / bell (`nf-fa-bell`).
pub const BELL: &str = "\u{f0f3}";

/// ATX header level 1 (`nf-md-format_header_1`) — drawn in place of the
/// `- ` bullet on a `# …` block. Levels 2–6 follow contiguously.
pub const HEADER_1: &str = "\u{f026b}";

/// ATX header level 2 (`nf-md-format_header_2`).
pub const HEADER_2: &str = "\u{f026c}";

/// ATX header level 3 (`nf-md-format_header_3`).
pub const HEADER_3: &str = "\u{f026d}";

/// ATX header level 4 (`nf-md-format_header_4`).
pub const HEADER_4: &str = "\u{f026e}";

/// ATX header level 5 (`nf-md-format_header_5`).
pub const HEADER_5: &str = "\u{f026f}";

/// ATX header level 6 (`nf-md-format_header_6`).
pub const HEADER_6: &str = "\u{f0270}";
