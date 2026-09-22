//! Pins `outl_actions::property::KNOWN_DATE_KEYS` against the
//! `DATE_KEYS` mirror in `@outl/shared`'s `properties.ts`.
//!
//! The list is a *convention* used only for UI affordances (the 📅
//! chip glyph, a client's date hint) — the query engine deliberately
//! ignores it and date-filters on any key whose value parses as an ISO
//! date. That split means the Rust and TS copies have no type system
//! tying them together, so a key added on one side silently changes
//! which keys look date-like to a user without the other following.
//!
//! Same reason `outl-theme/tests/tokens.rs` pins colour tokens to the
//! `Palette`: a hand-written mirror across the Rust/TS boundary is
//! only as safe as the test that refuses to let it drift.

use std::collections::HashSet;
use std::path::Path;

fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root must exist")
}

/// Pulls the string literals out of the single `export const DATE_KEYS = [ ... ]`
/// array in `properties.ts`, stopping at the first `]`.
fn ts_date_keys() -> Vec<String> {
    let path = workspace_root()
        .join("crates")
        .join("outl-frontend-shared")
        .join("src")
        .join("markdown")
        .join("properties.ts");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));

    let start = src
        .find("DATE_KEYS")
        .expect("properties.ts must declare DATE_KEYS");
    let after = &src[start..];
    let open = after.find('[').expect("DATE_KEYS must be an array literal");
    let body = &after[open + 1..];
    let close = body.find(']').expect("DATE_KEYS array must be closed");
    let body = &body[..close];

    let mut keys = Vec::new();
    let mut rest = body;
    while let Some(q) = rest.find('"') {
        rest = &rest[q + 1..];
        match rest.find('"') {
            Some(end) => {
                keys.push(rest[..end].to_string());
                rest = &rest[end + 1..];
            }
            None => break,
        }
    }
    keys
}

#[test]
fn the_ts_date_keys_mirror_the_rust_list() {
    let ts: HashSet<String> = ts_date_keys().into_iter().collect();
    let rust: HashSet<String> = outl_actions::KNOWN_DATE_KEYS
        .iter()
        .map(|k| k.to_string())
        .collect();

    assert!(
        ts == rust,
        "`DATE_KEYS` in outl-frontend-shared/src/markdown/properties.ts drifted from \
         `outl_actions::property::KNOWN_DATE_KEYS`.\n  \
         only-in-ts: {:?}\n  \
         only-in-rust: {:?}\n\
         The two are a hand-written mirror of one convention (which property keys look \
         date-like to a user). Add the key to BOTH lists, or remove it from both.",
        {
            let mut v: Vec<_> = ts.difference(&rust).collect();
            v.sort();
            v
        },
        {
            let mut v: Vec<_> = rust.difference(&ts).collect();
            v.sort();
            v
        },
    );
}
