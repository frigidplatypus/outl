//! Focus mode row. Same owner as `defaults.rs`, split out only
//! because that file sits at its size ceiling.

use super::chords::{shift_ctrl_ch, shift_meta_ch};
use crate::action::Action;
use crate::binding::{Binding, Mode};

/// `Cmd/Ctrl+Shift+F` toggles focus mode, the same switch as the
/// button in the top bar. Spelled for both OSes because the desktop
/// adapter never rewrites `Cmd` to `Ctrl`.
pub(super) fn bindings() -> Vec<Binding> {
    let label = "Toggle focus mode";
    vec![
        Binding::new(
            shift_meta_ch('f'),
            Mode::Global,
            Action::ToggleFocusMode,
            label,
        ),
        Binding::new(
            shift_ctrl_ch('f'),
            Mode::Global,
            Action::ToggleFocusMode,
            label,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use crate::action::Action;

    #[test]
    fn focus_mode_has_a_chord_on_both_oses() {
        let rows: Vec<_> = crate::default_bindings()
            .into_iter()
            .filter(|b| b.action == Action::ToggleFocusMode)
            .collect();
        assert_eq!(rows.len(), 2);
    }
}
