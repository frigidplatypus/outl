//! Stable identifiers for nodes and actors.
//!
//! Both are ULIDs (128-bit, lexicographically sortable). `NodeId` identifies
//! a block or page; `ActorId` identifies a device that produces ops.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// Stable identifier for a node in the outline tree.
///
/// Nodes are blocks and pages (a page is the root of its block tree).
/// Identifiers are ULIDs and must be globally unique across all replicas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(pub ulid::Ulid);

/// Lower bound of the plausible wall-clock window for
/// [`NodeId::created_ms`]: 2020-01-01T00:00:00Z in milliseconds — well
/// after any clock outl could plausibly run against, far below the
/// first ULIDs being minted.
const CREATED_FLOOR_MS: u64 = 1_577_836_800_000;

/// Upper bound: 2100-01-01T00:00:00Z in milliseconds.
const CREATED_CEIL_MS: u64 = 4_102_444_800_000;

impl NodeId {
    /// Generate a fresh `NodeId` using the current time and a random tail.
    pub fn new() -> Self {
        Self(ulid::Ulid::generate())
    }

    /// Creation time in milliseconds since the Unix epoch, decoded
    /// from the ULID's 48-bit timestamp, or `None` when the timestamp
    /// is not a plausible wall clock.
    ///
    /// The deterministic constructors ([`NodeId::from_slug`], the
    /// `from_seed` path callable-result blocks use) put a hash where
    /// the clock belongs, and hash bits land anywhere. The window
    /// `[2020-01-01, 2100-01-01)` covers every real outl id and is
    /// ~0.9% of the 48-bit space, so about 1 in 100 hash-derived ids
    /// still decodes to a fake timestamp — `Some` is best-effort, not
    /// proof of provenance. Callers that order by creation time
    /// should treat `None` as "unknown, sort last".
    pub fn created_ms(&self) -> Option<u64> {
        // The ULID timestamp is the top 48 bits; the remaining 80 are
        // entropy. Decoded by shift rather than a crate accessor so
        // the bit layout is stated in one place, like `from_bytes`.
        let ms = (u128::from(self.0) >> 80) as u64;
        (CREATED_FLOOR_MS..CREATED_CEIL_MS)
            .contains(&ms)
            .then_some(ms)
    }
    /// Deterministic [`NodeId`] derived from a page slug.
    ///
    /// A page (or journal) root's identity is its slug, not the wall
    /// clock. Two devices — or two code paths on one device (in-app
    /// creation, external-`.md` reconcile, desync recovery) —
    /// independently materialising the same slug must converge on the
    /// **same** node, or the day's content splits across two competing
    /// roots that iCloud / Syncthing can never merge (the CRDT only
    /// reconciles concurrent edits to the *same* node).
    ///
    /// Payload is `sha256("outl-page:" + slug)[..16]` as the ULID's
    /// 128-bit body. The constant prefix isolates this scheme from any
    /// other content-derived id. Output is stable across releases.
    ///
    /// This is the single owner of the derivation:
    /// `outl_actions::page::page_id_from_slug` and
    /// `outl_md::reconcile` both call through here so the three page-root
    /// creation paths cannot drift.
    pub fn from_slug(slug: &str) -> Self {
        Self::from_seed(b"outl-page:", slug)
    }

    /// Derive a deterministic [`NodeId`] from a domain `prefix` and a `seed`
    /// string: the ULID body is `sha256(prefix ++ seed)[..16]`. The prefix
    /// namespaces the scheme so two derivations (page-root ids, callable-result
    /// ids) can't collide. Same input always yields the same id, so two devices
    /// converge on it with no coordination. Stable across releases.
    ///
    /// Taking the first 16 bytes of SHA-256 leaves a 2^128 space, so the
    /// birthday-paradox collision risk is negligible for any real workspace
    /// (well under 10k pages).
    pub fn from_seed(prefix: &[u8], seed: &str) -> Self {
        let mut h = Sha256::new();
        h.update(prefix);
        h.update(seed.as_bytes());
        let digest = h.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest[..16]);
        Self(ulid::Ulid::from_bytes(bytes))
    }

    /// Returns the canonical [`NodeId`] for the workspace root.
    ///
    /// All real nodes descend (transitively) from this root.
    pub const fn root() -> Self {
        Self(ulid::Ulid(0))
    }

    /// Returns the canonical [`NodeId`] used to represent "deleted" nodes.
    ///
    /// We never physically remove a node from the op log; instead, deletion
    /// is implemented as a `Move` to `TRASH_ROOT`. See `docs/crdt.md`.
    pub const fn trash() -> Self {
        // Distinct sentinel value from ROOT (1u128).
        Self(ulid::Ulid(1))
    }
}

impl Default for NodeId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifier of a device (or process) producing ops.
///
/// ActorIds are ULIDs persisted per-workspace in `.outl/config.toml`.
/// They serve as the final tiebreak in HLC ordering.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ActorId(pub ulid::Ulid);

impl ActorId {
    /// Generate a fresh `ActorId`.
    pub fn new() -> Self {
        Self(ulid::Ulid::generate())
    }
}

impl Default for ActorId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_slug_is_deterministic() {
        // Same slug → same NodeId, on every call and every device.
        // This is what lets two reconcile / creation paths converge on
        // one page root instead of splitting the content in two.
        assert_eq!(
            NodeId::from_slug("2026-07-10"),
            NodeId::from_slug("2026-07-10")
        );
        assert_eq!(NodeId::from_slug("ideas"), NodeId::from_slug("ideas"));
    }

    #[test]
    fn from_slug_differs_by_slug() {
        assert_ne!(
            NodeId::from_slug("2026-07-10"),
            NodeId::from_slug("2026-07-11")
        );
    }

    #[test]
    fn from_slug_is_never_root_or_trash() {
        // A derived page id must not collide with the reserved sentinels.
        let id = NodeId::from_slug("2026-07-10");
        assert_ne!(id, NodeId::root());
        assert_ne!(id, NodeId::trash());
    }

    #[test]
    fn created_ms_decodes_a_minted_timestamp() {
        // A hand-built ULID carrying one plausible timestamp, so the
        // decode does not depend on the wall clock running the tests.
        let id = NodeId(ulid::Ulid::from_bytes(
            ((1_600_000_000_000u128 << 80) | 0xdeadbeef).to_be_bytes(),
        ));
        assert_eq!(id.created_ms(), Some(1_600_000_000_000));
        // A fresh id sits inside the window: sorting by creation time
        // must never park ordinary blocks last.
        assert!(NodeId::new().created_ms().is_some());
    }

    #[test]
    fn created_ms_rejects_non_clock_timestamps() {
        // The zero id, below the floor, and at the ceiling.
        assert_eq!(NodeId::root().created_ms(), None);
        let below = NodeId(ulid::Ulid::from_bytes(
            (1_000_000_000_000u128 << 80).to_be_bytes(),
        ));
        assert_eq!(below.created_ms(), None);
        let above = NodeId(ulid::Ulid::from_bytes(
            (5_000_000_000_000u128 << 80).to_be_bytes(),
        ));
        assert_eq!(above.created_ms(), None);
    }

    #[test]
    fn slug_derived_ids_usually_have_no_creation_time() {
        // `from_slug` puts hash bits where the clock belongs; the
        // window admits ~0.9% of them by construction. A handful of
        // slugs is a smoke test of the documented rate, not proof —
        // an occasional `Some` is the 1-in-100 caveat, not a bug.
        let none = ["notes", "ideas", "inbox", "meeting-notes", "recipes"]
            .iter()
            .filter(|s| NodeId::from_slug(s).created_ms().is_none())
            .count();
        assert!(none >= 3, "only {none}/5 slug ids decoded to None");
    }
}
