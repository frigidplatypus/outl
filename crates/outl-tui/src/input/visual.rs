//! Visual mode key handler.
//!
//! Visual mode operates on a contiguous range of outline blocks. Keys
//! that aren't `d`/`x`/`y`/`Tab`/`BackTab` either move the selection
//! (extending the range) or exit to Normal — except the `Ctrl+B`
//! backlinks toggle, which is pass-through chrome (the selection and
//! mode survive it).

use crate::state::App;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub(crate) fn handle_visual_key(app: &mut App, key: KeyEvent) -> Result<()> {
    match key.code {
        // Exit Visual via `exit_visual` so `last_visual` is captured —
        // a subsequent `gv` in Normal mode restores this range.
        KeyCode::Esc | KeyCode::Char('V') | KeyCode::Char('v') => app.exit_visual(),
        KeyCode::Char('d') | KeyCode::Char('x') => app.delete_visual_range(),
        KeyCode::Char('y') => app.yank_visual_range(),
        // `Tab` / `Shift-Tab` indent / outdent — vim ergonomics use
        // `>` / `<` for the same effect. Both fire the same range op
        // so muscle memory works either way; vim purists get `>`/`<`
        // without losing the `Tab` discoverability.
        KeyCode::Tab | KeyCode::Char('>') => app.indent_visual_range(),
        KeyCode::BackTab | KeyCode::Char('<') => app.outdent_visual_range(),
        // `Ctrl+B` toggles the backlinks panel exactly as in Normal —
        // mirrors the desktop, where the chord is `Global` scope and so
        // already fires in visual mode. Terminals collapse
        // `Ctrl+Shift+B` into `Ctrl+B`; accept either letter case.
        KeyCode::Char('b' | 'B') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.show_backlinks = !app.show_backlinks
        }
        // `Alt`+arrows drag the whole range among its siblings —
        // mirrors the single-block `Alt`+arrows in Normal mode. The
        // plain arrows below extend the selection, so `Alt` is what
        // separates "reorder the range" from "grow the range".
        KeyCode::Up if key.modifiers.contains(KeyModifiers::ALT) => app.move_up_visual_range(),
        KeyCode::Down if key.modifiers.contains(KeyModifiers::ALT) => app.move_down_visual_range(),
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::handle_visual_key;
    use crate::state::{App, Mode};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use outl_core::id::ActorId;
    use outl_core::workspace::Workspace;

    fn app_in_visual() -> (App, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let actor = ActorId::new();
        let ws = Workspace::open_in_memory(actor).unwrap();
        let mut app = App::new_for_tests(
            dir.path().to_path_buf(),
            ws,
            actor,
            crate::theme::default_theme(),
            false,
        )
        .unwrap();
        app.mode = Mode::Visual { anchor: 0 };
        (app, dir)
    }

    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    #[test]
    fn ctrl_b_toggles_backlinks_without_leaving_visual() {
        let (mut app, _dir) = app_in_visual();
        let before = app.show_backlinks;
        handle_visual_key(&mut app, ctrl('b')).unwrap();
        assert_eq!(app.show_backlinks, !before);
        // Pass-through chrome: the range selection and mode survive.
        assert!(matches!(app.mode, Mode::Visual { anchor: 0 }));
        handle_visual_key(&mut app, ctrl('B')).unwrap();
        assert_eq!(app.show_backlinks, before);
    }

    #[test]
    fn plain_b_does_not_toggle_backlinks() {
        let (mut app, _dir) = app_in_visual();
        let before = app.show_backlinks;
        handle_visual_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('b'), KeyModifiers::NONE),
        )
        .unwrap();
        assert_eq!(app.show_backlinks, before);
    }
}
