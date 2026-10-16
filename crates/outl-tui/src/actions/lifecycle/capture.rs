//! Land the captures `outl capture` left in the inbox (RFC 0363).
//!
//! The terminal is where `outl capture` is run from — by hand, from a
//! shell alias, from a macOS Service or a hotkey daemon — so the TUI not
//! draining would make the command's promise false for the client most
//! likely to be open. Every GUI client drains on mount and on every
//! return to the foreground; the TUI drains once, at boot, which is the
//! only moment it has (nothing here watches the inbox).
//!
//! `outl_actions::capture` owns the landing: the journal for the day the
//! capture was *made*, in capture order, idempotent on a crash. This
//! module is the three things around it the TUI has to do itself —
//! project the pages that gained blocks, reload the open view off that
//! projection, and say so.

use crate::state::{App, ToastKind};

impl App {
    /// Drain the workspace inbox and show what landed.
    ///
    /// Called once from the runtime, **after** `clock::init`: the day a
    /// capture belongs to is resolved in the configured timezone, so
    /// draining earlier would file a late-evening capture under the
    /// wrong day.
    ///
    /// Best-effort throughout. A failed drain leaves its file in the
    /// inbox for the next launch, which is the whole point of an inbox,
    /// so nothing here turns a capture into a failed boot.
    pub(crate) fn drain_capture_inbox(&mut self) {
        let dir = outl_actions::capture::inbox_in_workspace(&self.workspace_root);
        let report = match outl_actions::capture::drain_capture_dir(
            &mut self.workspace,
            &self.hlc,
            &dir,
            outl_actions::capture::capture_day,
        ) {
            Ok(report) => report,
            Err(e) => {
                self.toast(ToastKind::Warning, format!("capture inbox: {e}"));
                return;
            }
        };
        if report.landed.is_empty() {
            return;
        }

        // The TUI loads a view by parsing its `.md`, so the projection
        // has to happen before the reload or the blocks that just landed
        // are invisible until something else writes the page. Guarded,
        // like every other write path here: a page holding content the
        // op log never saw is refused rather than overwritten (root
        // invariant 8).
        let mut pages: Vec<_> = report.landed.iter().map(|(page, _)| *page).collect();
        pages.dedup();
        for page in pages {
            if let Err(e) = outl_actions::apply_page_md_with_sidecar_guarded(
                &self.workspace,
                &self.workspace_root,
                page,
            ) {
                // The ops are durable either way; what failed is the
                // projection. Naming it beats a silent stale page.
                self.toast(ToastKind::Warning, format!("capture projection: {e}"));
            }
        }
        self.refresh_workspace();

        let n = report.landed.len();
        self.toast(
            ToastKind::Info,
            if n == 1 {
                "added 1 capture to your journal".to_string()
            } else {
                format!("added {n} captures to your journal")
            },
        );
    }
}
