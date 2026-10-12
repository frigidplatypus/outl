//! Concurrent edits the CRDT merged, kept so the sync panel can show them
//! and offer a way back. RFC 0362, "Trust you can see".
//!
//! Detection runs at reload, with the old and the fresh tree in hand. A
//! block counts as merged when a newly arrived `Edit` from another actor
//! is *older* than an `Edit` this device had already made to the same
//! block: we wrote without having seen it, so the two raced and Yrs
//! merged them. A peer edit authored later than ours, without having
//! seen ours, is also a race but looks sequential from here; without
//! per-op causal context it cannot be told apart, and it is not listed.
//!
//! The log is device-local view state (root invariant 7 does not apply:
//! nothing here must converge), in memory, newest first, capped. Undo is
//! an ordinary `edit_text` back to this device's text from before the
//! merge, so it is new ops, syncs, and `Cmd+Z` can step it back.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use outl_core::hlc::Hlc;
use outl_core::id::{ActorId, NodeId};
use outl_core::op::Op;
use outl_core::workspace::Workspace;
use serde::Serialize;

use crate::helpers::{finish_in_page, parse_node_id};
use crate::host::AppHost;
use crate::state::PageView;

/// Entries kept per workspace.
pub const MERGE_LOG_CAP: usize = 50;

/// One merged concurrent edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MergedEditDto {
    /// Stable id for undo: the block id plus the arriving op's HLC.
    pub id: String,
    pub page_id: String,
    pub page_title: String,
    pub block_id: String,
    /// This device's text just before the merge.
    pub ours: String,
    /// The block's text after the merge.
    pub merged: String,
    /// Actor (op-log id, not a peer id) whose edit arrived.
    pub peer_actor: String,
    /// Wall clock of the arriving edit.
    pub at_ms: u64,
}

type Book = HashMap<PathBuf, Vec<MergedEditDto>>;

fn book() -> &'static Mutex<Book> {
    static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();
    BOOK.get_or_init(Mutex::default)
}

/// Blocks a reload merged concurrently, per the rule in the module doc.
pub fn detect(old: &Workspace, fresh: &Workspace, local: ActorId) -> Vec<MergedEditDto> {
    let seen: HashSet<Hlc> = old.log().iter().map(|op| op.ts).collect();
    let mut our_last: HashMap<NodeId, Hlc> = HashMap::new();
    for op in old.log().iter().filter(|op| op.actor == local) {
        if let Op::Edit { node, .. } = op.op {
            let slot = our_last.entry(node).or_insert(op.ts);
            *slot = (*slot).max(op.ts);
        }
    }
    let mut out = Vec::new();
    let mut listed = HashSet::new();
    for op in fresh.log().iter().filter(|op| !seen.contains(&op.ts)) {
        let Op::Edit { node, .. } = op.op else {
            continue;
        };
        if op.actor == local || listed.contains(&node) {
            continue;
        }
        if our_last.get(&node).is_none_or(|ours| *ours <= op.ts) {
            continue;
        }
        let ours = old.block_text(node).unwrap_or_default();
        let merged = fresh.block_text(node).unwrap_or_default();
        if ours == merged {
            continue;
        }
        let Some(page) = outl_actions::enclosing_page_id(fresh, node) else {
            continue;
        };
        let page_title = outl_actions::page_meta(fresh, page)
            .map(|m| m.title)
            .unwrap_or_default();
        listed.insert(node);
        out.push(MergedEditDto {
            id: format!("{node}@{}.{}", op.ts.physical_ms, op.ts.logical),
            page_id: page.to_string(),
            page_title,
            block_id: node.to_string(),
            ours,
            merged,
            peer_actor: op.actor.0.to_string(),
            at_ms: op.ts.physical_ms,
        });
    }
    out
}

/// Prepend `found` to the workspace's log, newest first, capped.
pub fn record(root: &Path, mut found: Vec<MergedEditDto>) {
    if found.is_empty() {
        return;
    }
    let Ok(mut book) = book().lock() else { return };
    let log = book.entry(root.to_path_buf()).or_default();
    found.sort_by_key(|b| std::cmp::Reverse(b.at_ms));
    found.append(log);
    found.truncate(MERGE_LOG_CAP);
    *log = found;
}

fn take(root: &Path, id: &str) -> Option<MergedEditDto> {
    let mut book = book().lock().ok()?;
    let log = book.get_mut(root)?;
    let at = log.iter().position(|e| e.id == id)?;
    Some(log.remove(at))
}

/// The merged edits recorded this session, newest first.
pub fn merged_edits<S: AppHost>(state: &S) -> Result<Vec<MergedEditDto>, String> {
    let root = state.storage_root()?;
    Ok(book()
        .lock()
        .ok()
        .and_then(|b| b.get(&root).cloned())
        .unwrap_or_default())
}

/// Put this device's pre-merge text back and drop the entry.
pub fn undo_merged_edit<S: AppHost>(state: &S, id: String) -> Result<PageView, String> {
    let root = state.storage_root()?;
    let entry = take(&root, &id).ok_or_else(|| "that merge is no longer listed".to_string())?;
    let page = parse_node_id(&entry.page_id)?;
    let node = parse_node_id(&entry.block_id)?;
    let hlc = state.hlc().clone();
    finish_in_page(state, page, |ws| {
        outl_actions::edit_text(ws, &hlc, node, &entry.ours)
    })
}

#[cfg(test)]
mod tests;
