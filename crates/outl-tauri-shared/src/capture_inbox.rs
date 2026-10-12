//! `drain_capture_inbox`: the only path from the iOS capture inbox
//! (RFC 0363) into the workspace. Each capture lands through
//! `outl_actions::capture` and is projected like any typed block, then
//! its file is deleted.

use std::fs;
use std::path::Path;

use outl_actions::capture::{
    capture_day, capture_page, inbox_from_env, land_capture, pending_captures,
};
use serde::Serialize;

use crate::helpers::{finish_in_page_with, with_ws_mut};
use crate::host::AppHost;

/// What a drain did, for the frontend to refresh and say so.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureDrainDto {
    /// Captures that became blocks.
    pub landed: usize,
    /// Files left in the inbox because they could not be read.
    pub skipped: usize,
    /// Journal pages that gained blocks, in landing order, deduplicated.
    pub page_ids: Vec<String>,
}

/// Drain the inbox the iOS shell pointed `OUTL_CAPTURE_INBOX` at. A
/// no-op returning zeros when there is no inbox (desktop, no App Group).
pub fn drain_capture_inbox<S: AppHost>(state: &S) -> Result<CaptureDrainDto, String> {
    match inbox_from_env() {
        Some(dir) => drain_capture_dir(state, &dir),
        None => Ok(CaptureDrainDto::default()),
    }
}

/// [`drain_capture_inbox`] for an explicit folder.
pub fn drain_capture_dir<S: AppHost>(state: &S, dir: &Path) -> Result<CaptureDrainDto, String> {
    let (pending, skipped) = pending_captures(dir);
    let mut out = CaptureDrainDto {
        skipped,
        ..CaptureDrainDto::default()
    };
    for capture in &pending {
        if capture.record.text.trim().is_empty() {
            let _ = fs::remove_file(&capture.path);
            continue;
        }
        let day = capture_day(capture.record.captured_at_ms);
        let page = with_ws_mut(state, |ws| {
            capture_page(ws, state.hlc(), day).map_err(|e| e.to_string())
        })?;
        finish_in_page_with(state, page, |ws| {
            land_capture(ws, state.hlc(), page, capture)
        })?;
        if let Err(e) = fs::remove_file(&capture.path) {
            tracing::warn!(
                "capture inbox: landed but kept {}: {e}",
                capture.path.display()
            );
        }
        out.landed += 1;
        let id = page.to_string();
        if !out.page_ids.contains(&id) {
            out.page_ids.push(id);
        }
    }
    Ok(out)
}
