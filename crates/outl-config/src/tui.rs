//! TUI-only preferences and icon selection.

use serde::{Deserialize, Serialize};

/// TUI-only preferences (the desktop ignores this section).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TuiCfg {
    /// Chrome icon set. Emoji is portable across ordinary terminal fonts;
    /// Nerd Font glyphs are available as an explicit opt-in.
    pub icons: TuiIconStyle,

    /// Capture the mouse so the app owns selection: drag across blocks
    /// selects a range and copies it as clean markdown on release, the
    /// scroll wheel moves the outline selection, a click selects a block.
    ///
    /// Default `false`, and deliberately opt-in: capturing the mouse
    /// **disables the terminal's own text selection** (selecting a URL,
    /// copying a single word, dragging across panes), which is muscle
    /// memory for many terminal users. Turn it on only if you want
    /// mouse-driven copy inside outl more than the terminal's native
    /// selection. The keyboard yank (`yy` / `Y` / Visual `y`) copies
    /// markdown to the clipboard regardless of this flag.
    pub mouse_capture: bool,

    /// How a standalone pipe table is framed in the pretty view. Read
    /// once at boot in `runtime.rs`; a pure display preference (same
    /// never-converges-between-devices policy as `theme.preset`, root
    /// `CLAUDE.md` invariant #7), so it never goes through the op log.
    /// Default [`TableStyle::Open`].
    pub table_style: TableStyle,
}

/// How the TUI draws a pipe table in the pretty view (RFC 0329).
///
/// `lowercase` serde so the TOML reads `table_style = "open"` /
/// `"box"` — the shape the user sees, not the Rust variant casing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TableStyle {
    /// Header + alignment rule + data rows, columns ruled with a dim `│`
    /// and no enclosing frame. The product default, and the only style
    /// a mid-prose / nested table ever uses.
    #[default]
    Open,
    /// The open grid wrapped in a full box — a top border, side walls on
    /// every row, and a bottom border. Applied only to a **standalone**
    /// table (the grid run is the whole block, at any indent level); a
    /// table sitting inside prose keeps the open style so its side walls
    /// never collide with the carrying block's indent rails.
    Box,
}

/// Icon set used by TUI chrome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum TuiIconStyle {
    /// Unicode emoji and symbols supported by ordinary terminal fonts.
    #[default]
    Emoji,
    /// Font Awesome / Material Design glyphs from a Nerd Font.
    NerdFont,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_icon_style_parses_and_defaults_to_emoji() {
        let c: crate::Config = toml::from_str("[tui]\nicons = \"nerd-font\"\n").unwrap();
        assert_eq!(c.tui.icons, TuiIconStyle::NerdFont);

        let c: crate::Config = toml::from_str("[theme]\npreset = \"nord\"\n").unwrap();
        assert_eq!(c.tui.icons, TuiIconStyle::Emoji);
    }

    #[test]
    fn table_style_defaults_to_open() {
        let c: crate::Config = toml::from_str("[tui]\nmouse_capture = true\n").unwrap();
        assert_eq!(c.tui.table_style, TableStyle::Open);
    }

    #[test]
    fn table_style_parses_box() {
        let c: crate::Config = toml::from_str("[tui]\ntable_style = \"box\"\n").unwrap();
        assert_eq!(c.tui.table_style, TableStyle::Box);
        let back = toml::to_string(&c).unwrap();
        assert!(back.contains("table_style = \"box\""), "{back}");
    }
}
