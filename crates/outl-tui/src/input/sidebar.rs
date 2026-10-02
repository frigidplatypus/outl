//! Event-loop-level `Global` chrome: the sidebar / backlinks chords and
//! every keystroke routed while focus is inside the sidebar.
//!
//! The desktop binds `Ctrl+Shift+E` / `Ctrl+Shift+B` at `Global` scope
//! in `outl-shortcuts`, so they fire in every editor state. The TUI's
//! counterpart is this module: the event loop calls [`handle_sidebar_key`]
//! before mode dispatch, so `Ctrl+E` opens a *focused* sidebar from
//! Normal, Insert, and Visual alike; while the sidebar holds focus its
//! keystrokes are routed here; and `Esc` hands the keyboard back to
//! whichever mode was active — an in-progress visual range or insert
//! buffer is untouched by the detour.

use crate::state::{App, Mode};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Handle the `Global`-scope sidebar / backlinks keystrokes.
///
/// Returns `true` when the key was consumed — the mode handler must not
/// see it.
pub(crate) fn handle_sidebar_key(app: &mut App, key: KeyEvent) -> Result<bool> {
    // The help popup owns the keyboard exclusively (same posture as
    // overlays): while it is up it — not the sidebar — drives keys,
    // and `?` stays reachable even when the sidebar holds focus.
    if app.show_help {
        return Ok(false);
    }
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // `Ctrl+B` toggles the backlinks panel. Terminals collapse
    // `Ctrl+Shift+B` into `Ctrl+B`; accept either letter case.
    if ctrl && matches!(key.code, KeyCode::Char('b' | 'B')) {
        app.show_backlinks = !app.show_backlinks;
        return Ok(true);
    }
    // `Ctrl+E` toggles the sidebar, opening with focus dropped on the
    // first non-empty section (Pinned by default) so `j/k`/`Enter`
    // work immediately — no extra Tab to "enter" it. Terminals
    // collapse `Ctrl+Shift+E` into `Ctrl+E`; both feel identical to
    // the desktop's `Cmd+Shift+E`. Pressing it while focused closes.
    if ctrl && matches!(key.code, KeyCode::Char('e' | 'E')) {
        if app.show_sidebar {
            app.sidebar_close();
        } else {
            app.sidebar_open_focused();
        }
        return Ok(true);
    }
    // While an overlay owns the keyboard, the chords above still
    // fire (Global scope, like the desktop with a picker open) but
    // sidebar *routing* declines — the picker's own input must get
    // Esc / arrows / typing, even if sidebar focus never left.
    if app.overlay.is_some() {
        return Ok(false);
    }
    if app.sidebar_focus.is_none() && app.pending_sidebar_delete.is_none() {
        return Ok(false);
    }
    // The delete-confirm prompt takes precedence while it is up.
    // `y` / `Y` confirms; anything else cancels and is
    // swallowed (the `pending_input_op` contract).
    if app.pending_sidebar_delete.is_some() {
        match key.code {
            KeyCode::Char('y' | 'Y') => app.sidebar_confirm_delete()?,
            _ => {
                app.pending_sidebar_delete = None;
                app.status.clear();
            }
        }
        return Ok(true);
    }
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.sidebar_move(1),
        KeyCode::Char('k') | KeyCode::Up => app.sidebar_move(-1),
        KeyCode::Char('g') => app.sidebar_cursor = 0,
        KeyCode::Char('G') => app.sidebar_move(i32::MAX / 2),
        KeyCode::Tab => app.sidebar_cycle_section(true),
        KeyCode::BackTab => app.sidebar_cycle_section(false),
        KeyCode::Enter => app.sidebar_activate()?,
        KeyCode::Char('d') => app.sidebar_delete_current(),
        KeyCode::Esc => app.sidebar_blur(),
        // Unlisted keys: in Normal they fall through (the outline
        // handler keeps working with the sidebar focused — `?` for
        // help, `q` to quit, plugin guard intact). In Insert / Visual
        // they are *swallowed*: the sidebar owns the keyboard, and a
        // stray `x` must never corrupt the insert buffer nor shrink
        // a visual range.
        _ => return Ok(!matches!(app.mode, Mode::Normal)),
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::handle_sidebar_key;
    use crate::edit_buffer::EditBuffer;
    use crate::state::{App, EditTarget, Mode, View};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use outl_core::id::ActorId;
    use outl_core::workspace::Workspace;
    use std::io::Write;

    fn app_in(mode: Mode) -> (App, tempfile::TempDir) {
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
        app.mode = mode;
        (app, dir)
    }

    fn ctrl(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::CONTROL)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn insert_mode() -> Mode {
        Mode::Insert {
            target: EditTarget::CurrentPage,
            block_path: vec![],
            buffer: EditBuffer::from_text("hello"),
            original_text: "hello".into(),
        }
    }

    /// A real page on disk for Enter-to-open tests: journal-shaped
    /// names would be misread as journal navigation.
    fn touch_recent(dir: &tempfile::TempDir, app: &mut App) {
        let mut f = std::fs::File::create(dir.path().join("alpha.md")).unwrap();
        f.write_all(b"- a\n").unwrap();
        // Boot opens today's journal, which records itself as the
        // most recent visit; drop it so row 0 is alpha.md.
        app.recent_paths.clear();
        app.recent_paths.push(dir.path().join("alpha.md"));
    }

    #[test]
    fn ctrl_e_opens_focused_sidebar_from_visual() {
        let (mut app, _dir) = app_in(Mode::Visual { anchor: 0 });
        assert!(handle_sidebar_key(&mut app, ctrl('e')).unwrap());
        assert!(app.show_sidebar);
        assert!(app.sidebar_focus.is_some());
        // Pass-through chrome: the range and mode survive the detour.
        assert!(matches!(app.mode, Mode::Visual { anchor: 0 }));
        assert!(handle_sidebar_key(&mut app, ctrl('E')).unwrap());
        assert!(!app.show_sidebar);
        assert!(matches!(app.mode, Mode::Visual { anchor: 0 }));
    }

    #[test]
    fn ctrl_e_opens_focused_sidebar_from_insert() {
        let (mut app, _dir) = app_in(insert_mode());
        assert!(handle_sidebar_key(&mut app, ctrl('e')).unwrap());
        assert!(app.sidebar_focus.is_some());
        // The typing buffer is untouched by the sidebar opening.
        let Mode::Insert { buffer, .. } = &app.mode else {
            panic!("insert mode must survive");
        };
        assert_eq!(buffer.as_string(), "hello");
    }

    #[test]
    fn ctrl_b_toggles_backlinks_from_any_mode() {
        let (mut app, _dir) = app_in(Mode::Visual { anchor: 0 });
        let before = app.show_backlinks;
        assert!(handle_sidebar_key(&mut app, ctrl('b')).unwrap());
        assert_eq!(app.show_backlinks, !before);
        assert!(matches!(app.mode, Mode::Visual { anchor: 0 }));
        assert!(handle_sidebar_key(&mut app, ctrl('B')).unwrap());
        assert_eq!(app.show_backlinks, before);
    }

    #[test]
    fn esc_blurs_and_leaves_mode_in_charge() {
        let (mut app, _dir) = app_in(Mode::Visual { anchor: 0 });
        handle_sidebar_key(&mut app, ctrl('e')).unwrap();
        assert!(handle_sidebar_key(&mut app, key(KeyCode::Esc)).unwrap());
        // Focus back in the outline, sidebar still drawn, mode intact.
        assert!(app.sidebar_focus.is_none());
        assert!(app.show_sidebar);
        assert!(matches!(app.mode, Mode::Visual { anchor: 0 }));
    }

    #[test]
    fn insert_keys_swallowed_while_focused() {
        let (mut app, _dir) = app_in(insert_mode());
        handle_sidebar_key(&mut app, ctrl('e')).unwrap();
        // `x` would delete a char in the insert buffer if it leaked.
        assert!(handle_sidebar_key(&mut app, key(KeyCode::Char('x'))).unwrap());
        let Mode::Insert { buffer, .. } = &app.mode else {
            panic!("insert mode must survive");
        };
        assert_eq!(buffer.as_string(), "hello");
    }

    #[test]
    fn normal_unlisted_keys_fall_through_while_focused() {
        let (mut app, _dir) = app_in(Mode::Normal);
        handle_sidebar_key(&mut app, ctrl('e')).unwrap();
        // `q` (and every other unlisted Normal key) must still reach
        // the outline handler — help, quit, plugin guard all live there.
        assert!(!handle_sidebar_key(&mut app, key(KeyCode::Char('q'))).unwrap());
    }

    #[test]
    fn enter_on_recent_page_opens_it_and_resets_mode() {
        let (mut app, dir) = app_in(Mode::Visual { anchor: 0 });
        touch_recent(&dir, &mut app);
        handle_sidebar_key(&mut app, ctrl('e')).unwrap();
        // Pinned is empty, Recent is not: focus starts on Recent.
        assert!(handle_sidebar_key(&mut app, key(KeyCode::Enter)).unwrap());
        assert!(matches!(app.view, View::Page(_)));
        // The opened page is now what the keyboard drives; the visual
        // range pointed at the *old* page's rows and dies with it.
        assert!(matches!(app.mode, Mode::Normal));
    }

    #[test]
    fn enter_on_calendar_keeps_mode() {
        // Boot records today's journal as the most-recent visit, so
        // without clearing it focus would land on Recent instead of
        // Calendar. Calendar's Enter is a no-op — the mode must NOT
        // be reset.
        let (mut app, _dir) = app_in(Mode::Visual { anchor: 0 });
        app.recent_paths.clear();
        handle_sidebar_key(&mut app, ctrl('e')).unwrap();
        assert_eq!(
            app.sidebar_focus,
            Some(crate::state::SidebarSection::Calendar)
        );
        assert!(handle_sidebar_key(&mut app, key(KeyCode::Enter)).unwrap());
        assert!(matches!(app.mode, Mode::Visual { anchor: 0 }));
    }

    #[test]
    fn delete_prompt_is_modal_while_focused() {
        let (mut app, dir) = app_in(Mode::Normal);
        touch_recent(&dir, &mut app);
        handle_sidebar_key(&mut app, ctrl('e')).unwrap();
        assert!(handle_sidebar_key(&mut app, key(KeyCode::Char('d'))).unwrap());
        assert!(app.pending_sidebar_delete.is_some());
        // While the prompt is up, `j` must not move the cursor — it
        // cancels and is swallowed instead.
        let cursor = app.sidebar_cursor;
        assert!(handle_sidebar_key(&mut app, key(KeyCode::Char('j'))).unwrap());
        assert!(app.pending_sidebar_delete.is_none());
        assert_eq!(app.sidebar_cursor, cursor);
    }

    #[test]
    fn help_popup_wins_over_sidebar_focus() {
        let (mut app, _dir) = app_in(Mode::Normal);
        handle_sidebar_key(&mut app, ctrl('e')).unwrap();
        app.show_help = true;
        // `?` reaches the help popup; sidebar routing stays out of it.
        assert!(!handle_sidebar_key(&mut app, key(KeyCode::Char('?'))).unwrap());
        assert!(!handle_sidebar_key(&mut app, ctrl('e')).unwrap());
    }
}
