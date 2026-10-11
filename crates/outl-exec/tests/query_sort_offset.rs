//! `sort:` direction, `journal-date` and `offset:` end-to-end through
//! the real index.
//!
//! The unit tests in `runtimes/query/engine.rs` hand-build `Hit`
//! values, so they prove the comparator and nothing about the rows the
//! engine feeds it. These drive the fence path: blocks projected from
//! a tree into a `WorkspaceIndex`, the DSL parsed from the fence body,
//! embeds out the other end — in the order `sort:` promises, after the
//! `offset:` skip.

#![cfg(feature = "lang-query")]

use std::path::{Path, PathBuf};

use outl_core::id::NodeId;
use outl_exec::{ExecContext, RuntimeRegistry};
use outl_md::block_index::IdentifiedNode;
use outl_md::index::{PageEntry, WorkspaceIndex};
use tempfile::TempDir;

/// One page declaration: slug and journal kind.
struct Page(&'static str, bool);

/// One block: hosting page slug, text, plus the `key:: value`
/// properties it carries.
struct Block(
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

/// Index over `pages`, one tree block per `blocks` entry. Returns the
/// index and the block ids in declaration order, so a test can name the
/// order `sort:` must produce.
fn index_with(pages: &[Page], blocks: &[Block], root: &Path) -> (WorkspaceIndex, Vec<NodeId>) {
    let mut idx = WorkspaceIndex::default();
    for Page(slug, is_journal) in pages {
        idx.insert_page(PageEntry {
            path: root.join(format!("pages/{slug}.md")),
            slug: (*slug).to_string(),
            title: (*slug).to_string(),
            icon: None,
            is_journal: *is_journal,
            pinned: false,
            page_type: None,
        });
    }
    let mut ids = Vec::new();
    let mut by_page: Vec<(&str, Vec<IdentifiedNode>)> = Vec::new();
    for Block(page_slug, text, props) in blocks {
        let node = IdentifiedNode {
            id: NodeId::new(),
            text: (*text).to_string(),
            properties: props
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            children: Vec::new(),
        };
        ids.push(node.id);
        match by_page.iter_mut().find(|(s, _)| s == page_slug) {
            Some((_, nodes)) => nodes.push(node),
            None => by_page.push((page_slug, vec![node])),
        }
    }
    for (slug, nodes) in &by_page {
        idx.collect_page_blocks_from_tree(slug, &root.join(format!("pages/{slug}.md")), nodes);
    }
    idx.collect_refs_from_indexed();
    (idx, ids)
}

/// Run `dsl` against `index` and return the matched ids in emission
/// order — these tests assert on the order `sort:` promises.
fn run(dsl: &str, index: &WorkspaceIndex, root: &Path) -> Vec<NodeId> {
    let runtime = RuntimeRegistry::with_builtins()
        .get("query")
        .expect("the query runtime is registered");
    let ctx = ExecContext {
        workspace_root: root.to_path_buf(),
        index: Some(index),
        ..Default::default()
    };
    let out = runtime.execute(dsl, &ctx).expect("the query runs");
    out.stdout
        .lines()
        .map(|line| {
            let handle = line
                .trim_start_matches("!((")
                .trim_end_matches("))")
                .to_string();
            index
                .resolve_block_ref(&handle)
                .unwrap_or_else(|| panic!("handle {handle} resolves"))
                .id
        })
        .collect()
}

fn empty_root() -> (TempDir, PathBuf) {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("pages")).unwrap();
    let root = dir.path().to_path_buf();
    (dir, root)
}

#[test]
fn sort_direction_and_offset_window_through_the_fenced_path() {
    let (_dir, root) = empty_root();
    let (index, ids) = index_with(
        &[Page("notes", false)],
        &[
            Block("notes", "TODO late", &[("due", "2025-09-01")]),
            Block("notes", "TODO early", &[("due", "2025-06-01")]),
            Block("notes", "TODO mid", &[("due", "2025-07-01")]),
        ],
        &root,
    );
    let [late, early, mid] = ids.try_into().unwrap();

    assert_eq!(run("sort: due", &index, &root), vec![early, mid, late]);
    assert_eq!(run("sort: due:desc", &index, &root), vec![late, mid, early]);
    assert_eq!(
        run("sort: due\noffset: 1\nlimit: 1", &index, &root),
        vec![mid]
    );
}

#[test]
fn journal_date_orders_journal_pages_and_parks_ordinary_pages_last() {
    let (_dir, root) = empty_root();
    let (index, ids) = index_with(
        &[
            Page("2025-07-03", true),
            Page("2025-06-30", true),
            Page("notes", false),
        ],
        &[
            Block("2025-07-03", "TODO july", &[]),
            Block("2025-06-30", "TODO june", &[]),
            Block("notes", "TODO plain", &[]),
        ],
        &root,
    );
    let [july, june, plain] = ids.try_into().unwrap();
    assert_eq!(
        run("sort: journal-date", &index, &root),
        vec![june, july, plain]
    );
}
