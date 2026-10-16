//! The capture inbox (RFC 0363): lines captured outside the app,
//! waiting to become blocks.
//!
//! Three shapes of writer, one format. An iOS widget, App Intent or
//! share extension, and an Android share target, write to the inbox a
//! mobile shell names in [`CAPTURE_INBOX_ENV`]; `outl capture` writes to
//! the workspace's own ([`inbox_in_workspace`]), which is what gives a
//! desktop every scriptable gesture it has.
//!
//! No writer touches the workspace. Each capture is one file
//! `<ULID>.json` in an inbox folder, written create-temp, fsync, rename
//! ([`write_capture`], or `OutlKit/Capture/CaptureInbox.swift` on iOS).
//! This module is the only path from that folder into the op log: it
//! lands each capture as a block at the end of the journal for the day it
//! was **made**, in ULID (capture) order, and deletes the file only after
//! the op is applied.
//!
//! Idempotent: the block id is the capture's ULID, so a drain that crashes
//! between the op and the delete finds the block already there and only
//! deletes the file. The ULID also carries the capture time on the block.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{NaiveDate, TimeZone, Utc};
use outl_core::hlc::HlcGenerator;
use outl_core::id::NodeId;
use outl_core::workspace::Workspace;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::block::create_with_explicit_id;
use crate::error::ActionError;
use crate::page::open_journal;
use crate::tree::position_for_new_last_child;

/// Environment variable the iOS shell sets to the inbox folder before the
/// app reads it. Unset (no App Group, desktop, tests) means no inbox.
pub const CAPTURE_INBOX_ENV: &str = "OUTL_CAPTURE_INBOX";

/// One capture file, as [`write_capture`] and the Swift writer both lay
/// it out.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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

/// Folder name an inbox goes by, in the App Group / `filesDir` container
/// a mobile shell points [`CAPTURE_INBOX_ENV`] at and under `.outl/` in a
/// workspace. Private: every caller wants a whole path, from
/// [`inbox_in_workspace`] or the variable, and the mobile shells cannot
/// read a Rust constant anyway (`OutlCapture.kt`, `CaptureInbox.swift`
/// spell it themselves).
const CAPTURE_DIR: &str = "captures";

/// The inbox folder from [`CAPTURE_INBOX_ENV`], when set and present.
///
/// This is the mobile one: an iOS extension can only reach the App Group
/// container, and an Android component only knows `filesDir`, so neither
/// can resolve which workspace is open. The shell creates the folder and
/// exports the variable before Rust starts.
pub fn inbox_from_env() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os(CAPTURE_INBOX_ENV)?);
    dir.is_dir().then_some(dir)
}

/// The workspace's own inbox, `<root>/.outl/captures`.
///
/// This is the one a process that *can* resolve a workspace writes to —
/// `outl capture`, and through it every scriptable gesture on a desktop
/// (a macOS Service or Shortcut, Raycast, a hotkey daemon). The capture
/// lands in the workspace it was aimed at, so nothing has to guess which
/// one a device-global folder meant.
///
/// `outl daily append` already writes such a line straight into the op
/// log, and is the better route when a second or two of workspace boot
/// is free. What the inbox buys is the other case: no boot at all, and
/// no op written under a second actor because the running app holds the
/// device's write actor (the `ops-<ulid>.jsonl` litter root `CLAUDE.md`
/// invariant 11 measures). The app drains it under its own actor.
///
/// Unlike the mobile inbox this folder is inside the workspace, so the
/// `file` transport (iCloud / Syncthing / a shared folder) can copy a
/// capture to another device before the first one drained it. Both then
/// drain it, and both emit `Create` for the **same** node id, because
/// the block id is the capture's ULID. `do_op(Create)` is idempotent, so
/// those converge on one block — the same property the crash replay in
/// RFC 0363 already leans on, reached by a second route.
pub fn inbox_in_workspace(root: &Path) -> PathBuf {
    root.join(".outl").join(CAPTURE_DIR)
}

/// Turn what a share sheet handed over into the one line that goes to
/// the inbox: the text, the URL, or both as `text url` when the URL is
/// not already in the text. Newlines collapse to spaces, because a
/// capture is one block.
///
/// `None` when there is nothing to save, which every caller words as
/// "Nothing to save" rather than writing an empty capture.
///
/// Mirrored in Swift as `OutlKit/Capture/SharedLine.swift`, for the same
/// reason the file format is: an iOS share extension cannot load this
/// library. Android reaches this one through
/// `Java_app_outl_mobile_1app_OutlCapture_write`, and `outl capture`
/// calls it directly, so Kotlin holds no copy of the rule.
pub fn shared_line(text: Option<&str>, url: Option<&str>) -> Option<String> {
    let body = text
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let link = url.unwrap_or_default().trim();
    match (body.is_empty(), link.is_empty()) {
        (true, true) => None,
        (true, false) => Some(link.to_string()),
        (false, true) => Some(body),
        (false, false) if body.contains(link) => Some(body),
        (false, false) => Some(format!("{body} {link}")),
    }
}

/// Write one capture into `dir` and return the file it published.
///
/// Create-temp, fsync, rename, so a drain racing this write sees a whole
/// capture or none of it: only the rename publishes the `<ULID>.json`
/// name [`pending_captures`] looks for, and the temp name is skipped
/// until then.
///
/// The ULID carries `captured_at_ms`, which is what makes the file name
/// the capture order and gives the landed block its capture time.
/// Whitespace-only text is refused, so a stray tap on a widget never
/// adds an empty block.
///
/// **Who calls this.** Android's share target, through
/// `Java_app_outl_mobile_1app_OutlCapture_write` — its components run in
/// the app's own process and load the Rust library, so the format has
/// one owner there. iOS cannot: a widget and a share extension are
/// separate processes under a memory ceiling that booting this library
/// would not fit, so they write the same layout from
/// `OutlKit/Capture/CaptureInbox.swift` (RFC 0363). That Swift writer is
/// the only second implementation of this format, which is why
/// [`CaptureRecord`]'s field names are spelled out on both sides.
pub fn write_capture(
    dir: &Path,
    text: &str,
    source: &str,
    captured_at_ms: i64,
) -> Result<PathBuf, ActionError> {
    let body = text.trim();
    if body.is_empty() {
        return Err(ActionError::EmptyCapture);
    }
    let id = Ulid::from_datetime(capture_instant(captured_at_ms));
    let record = CaptureRecord {
        text: body.to_string(),
        captured_at_ms,
        source: source.to_string(),
    };
    let bytes = serde_json::to_vec(&record).map_err(|e| ActionError::Io(e.into()))?;

    fs::create_dir_all(dir)?;
    // Leading dot so `capture_id` never reads it as a capture, and the
    // ULID so two writers cannot collide on the temp name either.
    let temp = dir.join(format!(".{id}.tmp"));
    let final_path = dir.join(format!("{id}.json"));
    let mut file = fs::File::create(&temp)?;
    if let Err(e) = file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&temp, &final_path))
    {
        let _ = fs::remove_file(&temp);
        return Err(e.into());
    }
    Ok(final_path)
}

/// The instant a capture stamped `ms` was made. A negative stamp (a
/// device clock before 1970) would wrap into a far-future ULID and sort
/// after everything, so it reads as the epoch instead.
fn capture_instant(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(u64::try_from(ms).unwrap_or(0))
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

/// Drain `dir`: land every capture in order through `land`, then delete
/// its file. `day_of` maps a capture time to its journal day (pass
/// [`capture_day`]). Stops at the first failed write, leaving that file
/// and the rest for the next drain.
///
/// `land` takes one capture and the day it belongs to, and returns the
/// page it landed in. It exists as a parameter because the two callers
/// need different locking around the same loop — the TUI has a plain
/// `&mut Workspace` ([`drain_capture_dir`] below), and the Tauri clients
/// take the workspace lock per capture and project the page
/// (`outl_tauri_shared::capture_inbox`). The *loop* is the part that must
/// not differ between them: which captures are skipped, that a file is
/// deleted only after its op, and in what order.
///
/// The landed block's id is not something `land` reports — it **is** the
/// capture's ULID, so the report derives it rather than taking a second
/// opinion on it.
pub fn drain_captures<E>(
    dir: &Path,
    day_of: impl Fn(i64) -> NaiveDate,
    mut land: impl FnMut(&PendingCapture, NaiveDate) -> Result<NodeId, E>,
) -> Result<DrainReport, E> {
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
        let page = land(capture, day_of(capture.record.captured_at_ms))?;
        if let Err(e) = fs::remove_file(&capture.path) {
            tracing::warn!(
                "capture inbox: landed but kept {}: {e}",
                capture.path.display()
            );
        }
        report.landed.push((page, NodeId(capture.id)));
    }
    Ok(report)
}

/// [`drain_captures`] against a workspace the caller already holds — the
/// TUI's launch drain, and every test in this module.
pub fn drain_capture_dir(
    workspace: &mut Workspace,
    hlc: &HlcGenerator,
    dir: &Path,
    day_of: impl Fn(i64) -> NaiveDate,
) -> Result<DrainReport, ActionError> {
    drain_captures(dir, day_of, |capture, day| {
        let page = capture_page(workspace, hlc, day)?;
        land_capture(workspace, hlc, page, capture)?;
        Ok(page)
    })
}

#[cfg(test)]
mod tests;
