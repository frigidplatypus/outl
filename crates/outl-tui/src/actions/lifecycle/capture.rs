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

use std::path::Path;
use std::sync::Arc;

use outl_actions::capture::{capture_day, capture_page, drain_captures, land_capture};
use outl_actions::{commit_page, ActionError, CommitHooks, SyncTransport};
use outl_core::hlc::HlcGenerator;
use outl_core::id::NodeId;
use outl_core::workspace::Workspace;

use crate::state::{App, ToastKind};

/// The TUI's half of `commit_page` for a landing capture: guarded
/// projection (root invariant 8), the peer announce, and a flag for the
/// backlink rebuild, which needs `&mut App` and so runs after the drain.
struct CaptureCommit<'a> {
    root: &'a Path,
    hlc: &'a HlcGenerator,
    transport: Option<&'a Arc<dyn SyncTransport>>,
    backlinks_stale: bool,
    projection_failure: Option<ActionError>,
}

impl CommitHooks for CaptureCommit<'_> {
    fn project(&mut self, workspace: &Workspace, page: NodeId) {
        if let Err(e) = outl_actions::apply_page_md_with_sidecar_guarded(workspace, self.root, page)
        {
            self.projection_failure = Some(e);
        }
    }

    fn invalidate_backlinks(&mut self) {
        self.backlinks_stale = true;
    }

    fn announce(&mut self, workspace: &Workspace, page: NodeId) {
        let Some(transport) = self.transport else {
            return;
        };
        if let Some(meta) = outl_actions::page_meta(workspace, page) {
            transport.announce_local_ops(&meta.slug, self.hlc.next());
        }
    }
}

impl App {
    /// Drain the workspace inbox and show what landed.
    ///
    /// Called once from the runtime, **after** `clock::init`: the day a
    /// capture belongs to is resolved in the configured timezone, so
    /// draining earlier would file a late-evening capture under the
    /// wrong day.
    ///
    /// Each capture goes through `commit_page`, so its page is projected
    /// and announced before `drain_captures` deletes its file. A later
    /// failure leaves that file and the rest for the next launch, and
    /// what already landed is still refreshed and reported.
    pub(crate) fn drain_capture_inbox(&mut self) {
        let dir = outl_actions::capture::inbox_in_workspace(&self.workspace_root);
        if outl_actions::capture::pending_captures(&dir).0.is_empty() {
            return;
        }
        // The open view may be the journal a capture lands in; snapshot
        // it so `u` takes the landing back like any other edit.
        self.snapshot_for_undo();

        let mut hooks = CaptureCommit {
            root: &self.workspace_root,
            hlc: &self.hlc,
            transport: self.sync_transport.as_ref(),
            backlinks_stale: false,
            projection_failure: None,
        };
        let workspace = &mut self.workspace;
        let hlc = &self.hlc;
        let mut landed = 0usize;
        let mut warnings = Vec::new();
        let result = drain_captures(&dir, capture_day, |capture, day| {
            let page = capture_page(workspace, hlc, day)?;
            commit_page(workspace, &mut hooks, page, |ws| {
                land_capture(ws, hlc, page, capture)
            })?;
            if let Some(e) = hooks.projection_failure.take() {
                // The op is durable; what failed is the projection.
                warnings.push(format!("capture projection: {e}"));
            }
            landed += 1;
            Ok::<_, ActionError>(page)
        });
        let backlinks_stale = hooks.backlinks_stale;
        if let Err(e) = result {
            warnings.push(format!("capture inbox: {e}"));
        }

        if landed > 0 {
            if backlinks_stale {
                self.spawn_backlink_index_rebuild();
            }
            self.refresh_workspace();
        }
        for w in warnings {
            self.toast(ToastKind::Warning, w);
        }
        if landed == 0 {
            self.undo.pop();
            return;
        }
        self.toast(
            ToastKind::Info,
            if landed == 1 {
                "added 1 capture to your journal".to_string()
            } else {
                format!("added {landed} captures to your journal")
            },
        );
    }
}
