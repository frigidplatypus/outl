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

fn write_capture(dir: &Path, ms: i64, text: &str) -> Ulid {
    let id = Ulid::from_parts(ms as u64, rand_tail(text));
    let body = serde_json::json!({ "text": text, "captured_at_ms": ms, "source": "intent" });
    fs::write(dir.join(format!("{id}.json")), body.to_string()).unwrap();
    id
}

fn rand_tail(seed: &str) -> u128 {
    seed.bytes()
        .fold(7u128, |a, b| a.wrapping_mul(31).wrapping_add(b as u128))
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
    write_capture(dir.path(), T0 + 3_000, "third");
    write_capture(dir.path(), T0 + 1_000, "first");
    write_capture(dir.path(), T0 + 2_000, "second");
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
    write_capture(dir.path(), T0, "call the venue");
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
    write_capture(dir.path(), T0 - DAY_MS, "from yesterday");
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
    write_capture(dir.path(), T0, "fine");
    let (mut w, hlc) = ws();

    let report = drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    assert_eq!(report.skipped, 1);
    assert_eq!(report.landed.len(), 1);
    assert!(bad.exists());
}

#[test]
fn the_block_id_is_the_capture_ulid() {
    let dir = tempfile::tempdir().unwrap();
    let id = write_capture(dir.path(), T0, "keep my time");
    let (mut w, hlc) = ws();

    let report = drain_capture_dir(&mut w, &hlc, dir.path(), day_of).unwrap();

    assert_eq!(report.landed[0].1, NodeId(id));
}
