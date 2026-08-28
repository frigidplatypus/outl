//! Nerd Font glyphs for TUI chrome.
//!
//! Every icon is a Font Awesome glyph from the set embedded in any
//! [Nerd Font](https://www.nerdfonts.com) build (`nf-fa-*`). The TUI
//! assumes the terminal runs a Nerd Font; on a font without the PUA
//! cells these render as tofu. That is the deliberate trade for not
//! shipping emoji in the UI.
//!
//! Codepoints verified against the Nerd Fonts 3.5.1 `glyphnames.json`
//! (`fa-*` entries), which embeds Font Awesome 4 at its original
//! codepoints.

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
