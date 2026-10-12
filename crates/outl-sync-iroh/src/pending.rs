//! What each paired device still has to receive from this one.
//!
//! The sync panel's "pending" column. Every completed exchange tells us
//! a floor for what the peer holds, per actor: its own advertised clock,
//! plus (on the initiator side, after the durable-ingest ack) everything
//! we held when we pushed. Ops this device holds above that floor are
//! the ones the peer has not got yet, as far as we can know.
//!
//! The floor lives in memory, per workspace and per peer, and is only
//! ever raised. A peer we have not exchanged with this session has no
//! floor, so it reports "unknown" rather than a number we would have to
//! invent. Counts are distinct ops per actor, the same unit the wire's
//! vector clock uses, so a snapshot boot does not skew them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::Result;
use outl_core::id::ActorId;

use crate::oplog::local_vector_clock;
use crate::protocol::{ActorClock, SyncRequest};

type Floor = HashMap<ActorId, u64>;
type Book = HashMap<PathBuf, HashMap<String, Floor>>;

fn book() -> &'static Mutex<Book> {
    static BOOK: OnceLock<Mutex<Book>> = OnceLock::new();
    BOOK.get_or_init(Mutex::default)
}

/// Raise the floor for `peer` with every clock in `clocks`.
pub(crate) fn record_known(
    workspace_root: &Path,
    peer: &str,
    clocks: &[&HashMap<ActorId, ActorClock>],
) {
    let Ok(mut book) = book().lock() else { return };
    let floor = book
        .entry(workspace_root.to_path_buf())
        .or_default()
        .entry(peer.to_string())
        .or_default();
    for clock in clocks {
        for (actor, c) in clock.iter() {
            let slot = floor.entry(*actor).or_insert(0);
            *slot = (*slot).max(c.count);
        }
    }
}

/// Initiator, after the durable-ingest ack: the peer holds its own clock
/// plus everything we held when we pushed.
pub(crate) fn pushed(
    root: &Path,
    peer: iroh::EndpointId,
    peer_clock: &HashMap<ActorId, ActorClock>,
    ours: &SyncRequest,
) {
    record_known(root, &peer.to_string(), &[peer_clock, &ours.vector_clock]);
}

/// Responder: the initiator's request clock is a floor for what it holds.
pub(crate) fn served(root: &Path, peer: iroh::EndpointId, theirs: &SyncRequest) {
    record_known(root, &peer.to_string(), &[&theirs.vector_clock]);
}

/// Ops in `local` above `floor`, summed across actors.
pub(crate) fn pending_count(local: &HashMap<ActorId, ActorClock>, floor: &Floor) -> u64 {
    local
        .iter()
        .map(|(actor, c)| {
            c.count
                .saturating_sub(floor.get(actor).copied().unwrap_or(0))
        })
        .sum()
}

/// Per peer node id, how many ops this device holds that the peer has not
/// confirmed. Peers with no exchange this session are absent.
pub fn pending_by_peer(workspace_root: &Path, actor: ActorId) -> Result<HashMap<String, u64>> {
    let floors: HashMap<String, Floor> = match book().lock() {
        Ok(book) => book.get(workspace_root).cloned().unwrap_or_default(),
        Err(_) => HashMap::new(),
    };
    if floors.is_empty() {
        return Ok(HashMap::new());
    }
    let local = local_vector_clock(&workspace_root.join("ops"), actor)?;
    Ok(floors
        .into_iter()
        .map(|(peer, floor)| (peer, pending_count(&local, &floor)))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use outl_core::hlc::Hlc;

    fn clock(entries: &[(ActorId, u64)]) -> HashMap<ActorId, ActorClock> {
        entries
            .iter()
            .map(|(a, n)| {
                (
                    *a,
                    ActorClock {
                        max: Hlc::new(0, 0, *a),
                        count: *n,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn counts_only_what_sits_above_the_floor() {
        let (a, b) = (ActorId::new(), ActorId::new());
        let local = clock(&[(a, 10), (b, 3)]);
        let floor: Floor = [(a, 7)].into_iter().collect();
        assert_eq!(pending_count(&local, &floor), 3 + 3);
    }

    #[test]
    fn a_peer_ahead_of_us_is_not_negative() {
        let a = ActorId::new();
        let floor: Floor = [(a, 9)].into_iter().collect();
        assert_eq!(pending_count(&clock(&[(a, 4)]), &floor), 0);
    }

    #[test]
    fn floor_only_rises_and_merges_both_clocks() {
        let root = PathBuf::from(format!("/tmp/outl-pending-test-{}", std::process::id()));
        let (a, b) = (ActorId::new(), ActorId::new());
        record_known(
            &root,
            "peer",
            &[&clock(&[(a, 5)]), &clock(&[(a, 2), (b, 4)])],
        );
        record_known(&root, "peer", &[&clock(&[(a, 1)])]);
        let floors = book().lock().unwrap().get(&root).cloned().unwrap();
        let floor = &floors["peer"];
        assert_eq!(floor[&a], 5);
        assert_eq!(floor[&b], 4);
    }

    #[test]
    fn unknown_workspace_reports_nothing() {
        let root = PathBuf::from("/tmp/outl-pending-never-synced");
        assert!(pending_by_peer(&root, ActorId::new()).unwrap().is_empty());
    }
}
