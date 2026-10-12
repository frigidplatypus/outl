//! The capture inbox (RFC 0363): lines captured outside the app by the
//! iOS widget, App Intent and share extension, waiting to become blocks.
//!
//! The extensions never touch the workspace. Each capture is one file
//! `<ULID>.json` in the inbox folder (App Group `captures/`), written
//! create-temp, fsync, rename. This module is the only path from that
//! folder into the op log: it lands each capture as a block at the end of
//! the journal for the day it was **made**, in ULID (capture) order, and
//! deletes the file only after the op is applied.
//!
//! Idempotent: the block id is the capture's ULID, so a drain that crashes
//! between the op and the delete finds the block already there and only
//! deletes the file. The ULID also carries the capture time on the block.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, TimeZone, Utc};
use outl_core::hlc::HlcGenerator;
use outl_core::id::NodeId;
use outl_core::workspace::Workspace;
use serde::Deserialize;
use ulid::Ulid;

use crate::block::create_with_explicit_id;
use crate::error::ActionError;
use crate::page::open_journal;
use crate::tree::position_for_new_last_child;

/// Environment variable the iOS shell sets to the inbox folder before the
/// app reads it. Unset (no App Group, desktop, tests) means no inbox.
pub const CAPTURE_INBOX_ENV: &str = "OUTL_CAPTURE_INBOX";

/// One capture file as the Swift writer lays it out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CaptureRecord {
    /// The captured line, as typed or shared.
    pub text: String,
    /// When it was captured (Unix ms); picks the journal day.
    pub captured_at_ms: i64,
    /// `widget`, `intent` or `share`.
    pub source: String,
}

/// A capture file found in the inbox, not yet landed.
#[derive(Debug, Clone)]
pub struct PendingCapture {
    /// The `<ULID>.json` file.
    pub path: PathBuf,
    /// The capture id, which becomes the block id.
    pub id: Ulid,
    /// What the file holds.
    pub record: CaptureRecord,
}

/// What one drain did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DrainReport {
    /// `(journal page, block)` per landed capture, in capture order.
    pub landed: Vec<(NodeId, NodeId)>,
    /// Files left in place because they could not be read or parsed.
    pub skipped: usize,
}

/// The inbox folder from [`CAPTURE_INBOX_ENV`], when set and present.
pub fn inbox_from_env() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os(CAPTURE_INBOX_ENV)?);
    dir.is_dir().then_some(dir)
}

/// Every complete capture in `dir`, in capture (ULID) order. Temp files
/// (anything not named `<ULID>.json`) are not captures yet and are left
/// for the next drain. Unreadable or malformed files are counted in
/// `skipped` and left in place.
pub fn pending_captures(dir: &Path) -> (Vec<PendingCapture>, usize) {
    let Ok(entries) = fs::read_dir(dir) else {
        return (Vec::new(), 0);
    };
    let mut found = Vec::new();
    let mut skipped = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(id) = capture_id(&path) else {
            continue;
        };
        match fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<CaptureRecord>(&b).ok())
        {
            Some(record) => found.push(PendingCapture { path, id, record }),
            None => {
                tracing::warn!("capture inbox: unreadable capture {}", path.display());
                skipped += 1;
            }
        }
    }
    found.sort_by_key(|c| c.id);
    (found, skipped)
}

fn capture_id(path: &Path) -> Option<Ulid> {
    if path.extension()? != "json" {
        return None;
    }
    Ulid::from_string(path.file_stem()?.to_str()?).ok()
}

/// The journal day a capture made at `ms` belongs to, in the configured
/// timezone (OS local when none is set).
pub fn capture_day(ms: i64) -> NaiveDate {
    let at = Utc
        .timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now);
    match crate::clock::timezone() {
        Some(tz) => at.with_timezone(&tz).date_naive(),
        None => at.with_timezone(&chrono::Local).date_naive(),
    }
}

/// The journal a capture lands in (created when missing).
pub fn capture_page(
    workspace: &mut Workspace,
    hlc: &HlcGenerator,
    day: NaiveDate,
) -> Result<NodeId, ActionError> {
    open_journal(workspace, hlc, day)
}

/// Land one capture as the last block of `page`. A capture whose block is
/// already in the tree (a replay after a crash) is left as it is.
pub fn land_capture(
    workspace: &mut Workspace,
    hlc: &HlcGenerator,
    page: NodeId,
    capture: &PendingCapture,
) -> Result<NodeId, ActionError> {
    let block = NodeId(capture.id);
    if workspace.tree().contains(block) {
        return Ok(block);
    }
    let position = position_for_new_last_child(workspace, page);
    create_with_explicit_id(
        workspace,
        hlc,
        block,
        page,
        position,
        Some(capture.record.text.as_str()),
    )
}

/// Drain `dir` into `workspace`: land every capture in order, then delete
/// its file. `day_of` maps a capture time to its journal day (pass
/// [`capture_day`]). Stops at the first failed write, leaving that file
/// and the rest for the next drain.
pub fn drain_capture_dir(
    workspace: &mut Workspace,
    hlc: &HlcGenerator,
    dir: &Path,
    day_of: impl Fn(i64) -> NaiveDate,
) -> Result<DrainReport, ActionError> {
    let (pending, skipped) = pending_captures(dir);
    let mut report = DrainReport {
        skipped,
        ..DrainReport::default()
    };
    for capture in &pending {
        if capture.record.text.trim().is_empty() {
            let _ = fs::remove_file(&capture.path);
            continue;
        }
        let page = capture_page(workspace, hlc, day_of(capture.record.captured_at_ms))?;
        let block = land_capture(workspace, hlc, page, capture)?;
        if let Err(e) = fs::remove_file(&capture.path) {
            tracing::warn!(
                "capture inbox: landed but kept {}: {e}",
                capture.path.display()
            );
        }
        report.landed.push((page, block));
    }
    Ok(report)
}

#[cfg(test)]
mod tests;
