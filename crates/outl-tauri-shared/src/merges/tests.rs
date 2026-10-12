use super::*;
use outl_actions::{append_block, edit_text, open_or_create_page as open_or_create, PageKind};
use outl_core::hlc::HlcGenerator;

fn replay_into(ws: &Workspace, into: &mut Workspace) {
    let have: HashSet<Hlc> = into.log().iter().map(|op| op.ts).collect();
    let ops: Vec<_> = ws
        .log()
        .iter()
        .filter(|op| !have.contains(&op.ts))
        .cloned()
        .collect();
    for op in ops {
        into.apply(op).unwrap();
    }
}

/// Two devices share a block; each edits it before seeing the other.
/// The peer's edit is stamped earlier, then arrives here.
fn race() -> (Workspace, Workspace, ActorId, NodeId) {
    let me = ActorId::new();
    let peer = ActorId::new();
    let peer_hlc = HlcGenerator::new(peer);
    let my_hlc = HlcGenerator::new(me);

    let mut theirs = Workspace::open_in_memory(peer).unwrap();
    let page = open_or_create(&mut theirs, &peer_hlc, "alpha", "Alpha", PageKind::Page).unwrap();
    let block = append_block(&mut theirs, &peer_hlc, Some(page), Some("milk")).unwrap();

    let mut mine = Workspace::open_in_memory(me).unwrap();
    replay_into(&theirs, &mut mine);

    edit_text(&mut theirs, &peer_hlc, block, "milk and eggs").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(3));
    edit_text(&mut mine, &my_hlc, block, "oat milk").unwrap();

    let old = {
        let mut copy = Workspace::open_in_memory(me).unwrap();
        replay_into(&mine, &mut copy);
        copy
    };
    replay_into(&theirs, &mut mine);
    (old, mine, me, block)
}

#[test]
fn a_racing_peer_edit_is_listed_with_both_texts() {
    let (old, fresh, me, block) = race();
    let found = detect(&old, &fresh, me);
    assert_eq!(found.len(), 1);
    let e = &found[0];
    assert_eq!(e.block_id, block.to_string());
    assert_eq!(e.ours, "oat milk");
    assert_ne!(e.merged, e.ours);
    assert_eq!(e.page_title, "Alpha");
}

#[test]
fn a_peer_edit_with_no_local_edit_is_not_a_merge() {
    let me = ActorId::new();
    let peer = ActorId::new();
    let peer_hlc = HlcGenerator::new(peer);
    let mut theirs = Workspace::open_in_memory(peer).unwrap();
    let page = open_or_create(&mut theirs, &peer_hlc, "alpha", "Alpha", PageKind::Page).unwrap();
    let block = append_block(&mut theirs, &peer_hlc, Some(page), Some("a")).unwrap();
    let mut old = Workspace::open_in_memory(me).unwrap();
    replay_into(&theirs, &mut old);
    edit_text(&mut theirs, &peer_hlc, block, "a b").unwrap();
    let mut fresh = Workspace::open_in_memory(me).unwrap();
    replay_into(&theirs, &mut fresh);
    assert!(detect(&old, &fresh, me).is_empty());
}

#[test]
fn the_log_is_newest_first_and_capped() {
    let root = PathBuf::from(format!("/tmp/outl-merges-{}", std::process::id()));
    let entry = |n: u64| MergedEditDto {
        id: n.to_string(),
        page_id: String::new(),
        page_title: String::new(),
        block_id: String::new(),
        ours: String::new(),
        merged: String::new(),
        peer_actor: String::new(),
        at_ms: n,
    };
    record(&root, (0..40).map(entry).collect());
    record(&root, (100..120).map(entry).collect());
    let log = book().lock().unwrap().get(&root).cloned().unwrap();
    assert_eq!(log.len(), MERGE_LOG_CAP);
    assert_eq!(log[0].at_ms, 119);
    assert!(take(&root, "119").is_some());
    assert!(take(&root, "119").is_none(), "undo removes the entry");
}
