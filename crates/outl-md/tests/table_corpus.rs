//! The pipe-table parser pinned against a shared corpus.
//!
//! `corpus/table_grid.json` is a single fixture both suites read:
//! this test regenerates the `expected` grids from `outl-md`'s
//! `parse_table_block` (with `OUTL_UPDATE_TABLE_FIXTURE=1`), and the
//! vitest suite in `outl-frontend-shared` asserts the TypeScript port
//! (`src/markdown/table.ts`) recovers the identical grid from the same
//! `text`. One fixture, two implementations, no drift — the same
//! arrangement `corpus_gate.rs` is for whole documents, narrowed to
//! the table shape the GUI grid renderer (`TableGrid`) depends on.
//!
//! Run `OUTL_UPDATE_TABLE_FIXTURE=1 cargo test -p outl-md --test
//! table_corpus` to regenerate after intentionally changing the Rust
//! parser; the vitest suite then fails until the port follows.

use std::path::Path;

use outl_md::table::parse_table_block;
use serde::{Deserialize, Serialize};

/// One corpus case: a block's stored text and the grid the parser is
/// expected to recover (`None`/`null` = the text is not a whole table).
#[derive(Serialize, Deserialize)]
struct Case {
    name: String,
    text: String,
    expected: Option<outl_md::table::Table>,
}

fn fixture() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/corpus/table_grid.json"))
}

fn cases() -> Vec<Case> {
    let raw = std::fs::read_to_string(fixture())
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", fixture().display()));
    serde_json::from_str(&raw).expect("table_grid.json must be a JSON array of cases")
}

/// Hand-curated inputs; the `expected` field is generated. Keeping the
/// inputs here (not in the JSON) means regeneration can never invent a
/// case, and the JSON stays a pure outputs-of-the-parser record.
fn inputs() -> Vec<(&'static str, &'static str)> {
    vec![
        (
            "plain",
            "|a|b|\n|---|---|\n|1|2|\n|3|4|",
        ),
        (
            "alignment",
            "|l|c|r|\n|:---|:---:|---:|\n|x|y|z|",
        ),
        (
            "padded-and-ragged",
            "|  a  |  b  |\n| --- | --- |\n|1|\n|2|3|4|",
        ),
        (
            "escaped-pipe",
            "|a|b|\n|---|---|\n|x \\| y|z|",
        ),
        (
            "empty-cells",
            "|a|b|\n|---|---|\n||2|",
        ),
        (
            "untrimmed-delimiter",
            "|a|b|\n  |---|---|  \n|1|2|",
        ),
        (
            "prose-then-table",
            "look:\n|a|b|\n|---|---|\n|1|2|",
        ),
        (
            "table-then-prose",
            "|a|b|\n|---|---|\n|1|2|\nthat was the table",
        ),
        (
            "quoted-table",
            "> |a|b|\n> |---|---|\n> |1|2|",
        ),
        (
            "header-chrome",
            "## |a|b|\n|---|---|\n|1|2|",
        ),
        (
            "header-only",
            "|a|b|\n|---|---|",
        ),
        (
            "delimiter-first",
            "|---|---|\n|1|2|",
        ),
        (
            "no-delimiter",
            "|a|b|\n|1|2|",
        ),
        (
            "pipe-in-prose",
            "a | b but not a table",
        ),
        (
            "empty",
            "",
        ),
        (
            "cjk-width",
            "|名前|説明|\n|---|---|\n|あ|表|\n|bb|cc|",
        ),
        (
            "single-column",
            "|a|\n|---|\n|1|\n|2|",
        ),
    ]
}

#[test]
fn parser_matches_the_shared_fixture() {
    let on_disk = cases();
    let inputs = inputs();
    assert_eq!(
        on_disk.len(),
        inputs.len(),
        "the fixture and the input list moved apart — regenerate with \
         OUTL_UPDATE_TABLE_FIXTURE=1 cargo test -p outl-md --test table_corpus"
    );
    for ((name, text), case) in inputs.iter().zip(&on_disk) {
        assert_eq!(&case.name, name, "fixture order changed");
        assert_eq!(&case.text, text, "fixture input for {name} changed");
        assert_eq!(
            parse_table_block(text).as_ref(),
            case.expected.as_ref(),
            "parse_table_block disagrees with the fixture for {name} — if the \
             parser change is intentional, regenerate the fixture and update \
             the TypeScript port in the same PR",
        );
    }
}

#[test]
fn regenerate_fixture() {
    if std::env::var_os("OUTL_UPDATE_TABLE_FIXTURE").is_none() {
        return;
    }
    let out: Vec<Case> = inputs()
        .into_iter()
        .map(|(name, text)| Case {
            name: name.to_string(),
            text: text.to_string(),
            expected: parse_table_block(text),
        })
        .collect();
    let json = serde_json::to_string_pretty(&out).unwrap();
    std::fs::write(fixture(), format!("{json}\n")).unwrap();
}
