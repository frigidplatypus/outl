use std::fs;
use std::path::Path;

use chrono::NaiveDate;
use outl_core::hlc::HlcGenerator;
use outl_core::id::{ActorId, NodeId};
use outl_core::workspace::Workspace;
use ulid::Ulid;

use super::*;
use crate::dates::journal_slug;
use crate::page::find_by_slug;
use crate::tree::children_of;

fn ws() -> (Workspace, HlcGenerator) {
    let actor = ActorId::new();
    (
        Workspace::open_in_memory(actor).unwrap(),
        HlcGenerator::new(actor),
    )
}

const DAY_MS: i64 = 86_400_000;
const T0: i64 = 1_760_000_000_000;

fn day_of(ms: i64) -> NaiveDate {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap()
        .date_naive()
}

/// Put one capture in the inbox through the real writer, so these tests
/// read what an extension actually leaves behind rather than a second
/// hand-written spelling of the same format.
fn stage_capture(dir: &Path, ms: i64, text: &str) -> Ulid {
    let path = write_capture(dir, text, "intent", ms).unwrap();
    capture_id(&path).expect("the writer publishes a <ULID>.json name")
}

fn texts(w: &Workspace, page: NodeId) -> Vec<String> {
    children_of(w, page)
        .into_iter()
        .map(|(id, _)| w.block_text(id).unwrap_or_default())
        .collect()
}

#[test]
fn drain_appends_in_capture_order() {
    let dir = tempfile::tempdir().unwrap();
    // Written newest first, so mtime order is the reverse of capture order.
    stage_capture(dir.path(), T0 + 3_000, "third");
    stage_capture(dir.path(), T0 + 1_000, "first");
    stage_capture(dir.path(), T0 + 2_000, "second");
    let (mut w, hlc) = ws();

    let report = drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    let page = find_by_slug(&w, &journal_slug(day_of(T0))).unwrap();
    assert_eq!(texts(&w, page), ["first", "second", "third"]);
    assert_eq!(report.landed.len(), 3);
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        0,
        "landed files are deleted"
    );
}

#[test]
fn drain_is_idempotent_after_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    stage_capture(dir.path(), T0, "call the venue");
    let (mut w, hlc) = ws();
    // The op committed, then the process died before the delete.
    let (pending, _) = pending_captures(dir.path());
    let page = capture_page(&mut w, &hlc, day_of(T0)).unwrap();
    land_capture(&mut w, &hlc, page, &pending[0]).unwrap();

    drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    assert_eq!(texts(&w, page), ["call the venue"]);
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn capture_lands_in_the_day_it_was_made() {
    let dir = tempfile::tempdir().unwrap();
    stage_capture(dir.path(), T0 - DAY_MS, "from yesterday");
    let (mut w, hlc) = ws();

    drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    let yesterday = find_by_slug(&w, &journal_slug(day_of(T0 - DAY_MS))).unwrap();
    assert_eq!(texts(&w, yesterday), ["from yesterday"]);
    assert!(find_by_slug(&w, &journal_slug(day_of(T0))).is_none());
}

#[test]
fn partial_file_is_ignored() {
    let dir = tempfile::tempdir().unwrap();
    let tmp = dir
        .path()
        .join(format!(".{}.tmp", Ulid::from_parts(T0 as u64, 99)));
    fs::write(&tmp, "{\"text\":\"half").unwrap();
    let (mut w, hlc) = ws();

    let report = drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    assert!(report.landed.is_empty());
    assert!(tmp.exists(), "a temp file is left for the next drain");
}

#[test]
fn a_malformed_capture_is_kept_and_counted() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir
        .path()
        .join(format!("{}.json", Ulid::from_parts(T0 as u64, 99)));
    fs::write(&bad, "not json").unwrap();
    stage_capture(dir.path(), T0, "fine");
    let (mut w, hlc) = ws();

    let report = drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    assert_eq!(report.skipped, 1);
    assert_eq!(report.landed.len(), 1);
    assert!(bad.exists());
}

#[test]
fn the_block_id_is_the_capture_ulid() {
    let dir = tempfile::tempdir().unwrap();
    let id = stage_capture(dir.path(), T0, "keep my time");
    let (mut w, hlc) = ws();

    let report = drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    // Asked of the tree, not of the report: the report *derives* the id
    // from the capture, so reading it back from there would assert that
    // one line against itself. What has to be true is that the block in
    // the workspace is that id — it is what makes a replay after a crash
    // land on the same block instead of a second one.
    assert_eq!(report.landed[0].1, NodeId(id));
    assert!(w.tree().contains(NodeId(id)), "the block is the capture id");
    assert_eq!(w.block_text(NodeId(id)).as_deref(), Some("keep my time"));
}

#[test]
fn a_written_capture_reads_back_whole() {
    let dir = tempfile::tempdir().unwrap();

    write_capture(dir.path(), "  ring the dentist  ", "share", T0).unwrap();

    let (pending, skipped) = pending_captures(dir.path());
    assert_eq!(skipped, 0);
    assert_eq!(
        pending[0].record,
        CaptureRecord {
            text: "ring the dentist".to_string(),
            captured_at_ms: T0,
            source: "share".to_string(),
        },
        "the writer trims and the reader sees every field"
    );
}

#[test]
fn written_captures_sort_in_capture_order() {
    let dir = tempfile::tempdir().unwrap();
    // Written newest first: only the stamp in the name may decide.
    write_capture(dir.path(), "third", "share", T0 + 3_000).unwrap();
    write_capture(dir.path(), "first", "share", T0 + 1_000).unwrap();
    write_capture(dir.path(), "second", "share", T0 + 2_000).unwrap();

    let (pending, _) = pending_captures(dir.path());

    let texts: Vec<_> = pending.iter().map(|c| c.record.text.as_str()).collect();
    assert_eq!(texts, ["first", "second", "third"]);
}

#[test]
fn a_blank_capture_is_refused_at_the_writer() {
    let dir = tempfile::tempdir().unwrap();

    let refused = write_capture(dir.path(), "   \n\t ", "share", T0);

    assert!(matches!(refused, Err(ActionError::EmptyCapture)));
    assert_eq!(
        fs::read_dir(dir.path()).unwrap().count(),
        0,
        "nothing is published, not even a temp file"
    );
}

#[test]
fn the_writer_leaves_only_the_published_name_behind() {
    let dir = tempfile::tempdir().unwrap();

    let path = write_capture(dir.path(), "one line", "share", T0).unwrap();

    let names: Vec<_> = fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        names,
        [path.file_name().unwrap().to_string_lossy().into_owned()],
        "the temp file is renamed, never left next to the capture"
    );
}

#[test]
fn the_writer_creates_an_inbox_that_is_not_there_yet() {
    let dir = tempfile::tempdir().unwrap();
    let inbox = dir.path().join("captures");

    write_capture(&inbox, "first ever capture", "share", T0).unwrap();

    let (pending, _) = pending_captures(&inbox);
    assert_eq!(pending.len(), 1);
}

#[test]
fn a_capture_written_before_1970_still_sorts_first() {
    let dir = tempfile::tempdir().unwrap();
    // A device clock behind the epoch: the stamp must not wrap into a
    // far-future ULID that sorts after every real capture.
    write_capture(dir.path(), "clock was wrong", "share", -5_000).unwrap();
    write_capture(dir.path(), "clock was right", "share", T0).unwrap();

    let (pending, _) = pending_captures(dir.path());

    let texts: Vec<_> = pending.iter().map(|c| c.record.text.as_str()).collect();
    assert_eq!(texts, ["clock was wrong", "clock was right"]);
}

#[test]
fn a_share_becomes_one_line() {
    // Every case `SharedLine.swift` answers, so the two cannot drift
    // without one of them failing a test.
    assert_eq!(shared_line(None, None), None);
    assert_eq!(shared_line(Some("  \n\t"), None), None);
    assert_eq!(
        shared_line(Some("read this"), None).as_deref(),
        Some("read this")
    );
    assert_eq!(
        shared_line(None, Some("https://outl.app")).as_deref(),
        Some("https://outl.app")
    );
    assert_eq!(
        shared_line(Some("read this"), Some("https://outl.app")).as_deref(),
        Some("read this https://outl.app"),
        "a link not already in the text is appended"
    );
    assert_eq!(
        shared_line(Some("see https://outl.app now"), Some("https://outl.app")).as_deref(),
        Some("see https://outl.app now"),
        "a link already in the text is not repeated"
    );
    assert_eq!(
        shared_line(Some("first\n\n  second  \nthird"), None).as_deref(),
        Some("first second third"),
        "a capture is one block, so newlines collapse"
    );
}
