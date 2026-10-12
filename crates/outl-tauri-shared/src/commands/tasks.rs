//! Every open task in the workspace, as one list.
//!
//! "What do I still have to do" was answerable only by writing a
//! ` ```query ` block with `status: open` on some page. Mobile now has a
//! Tasks tab that asks it directly, and this is the command behind it.
//!
//! The marker vocabulary is `outl_actions::split_todo`'s, so `TODO`,
//! `DOING` and the `[ ]` checkbox spelling all count, exactly as they
//! do for the checkbox a client draws. The list reads the projected
//! `.md` files the same way `search_blocks` does, which keeps trashed
//! blocks out by construction.

use serde::Serialize;

use outl_actions::{split_todo, TodoState};
use outl_md::block_index::BlockEntry;
use outl_md::index::WorkspaceIndex;

use crate::host::AppHost;

/// Rows sent to the client. The total is never capped, so a workspace
/// with more open tasks than this says so instead of looking complete.
pub const TASK_LIMIT: usize = 200;

/// One open task on the wire.
#[derive(Debug, Clone, Serialize)]
pub struct TaskHit {
    /// Node id, so the client can zoom into the block once its page is open.
    pub id: String,
    /// Slug of the page hosting the task.
    pub source_slug: String,
    /// `todo` or `doing`.
    pub status: String,
    /// Block text with the task marker stripped, on one line.
    pub text: String,
}

/// The open tasks plus the count they were cut from.
#[derive(Debug, Clone, Serialize)]
pub struct OpenTasks {
    /// Every open task in the workspace, before the cap.
    pub total: usize,
    /// At most [`TASK_LIMIT`] rows: started tasks first, then the most
    /// recent journal days, then pages, each page in outline order.
    pub tasks: Vec<TaskHit>,
}

/// List the workspace's open tasks.
pub fn list_open_tasks<S: AppHost>(state: &S) -> Result<OpenTasks, String> {
    let root = state.storage_root()?;
    let index = WorkspaceIndex::build(&root);
    Ok(collect_open_tasks(index.iter_blocks(), TASK_LIMIT))
}

/// The selection and ordering behind [`list_open_tasks`], split out so
/// it can be tested without a workspace on disk.
pub fn collect_open_tasks<'a>(
    blocks: impl Iterator<Item = &'a BlockEntry>,
    limit: usize,
) -> OpenTasks {
    let mut open: Vec<(TodoState, &BlockEntry, &str)> = blocks
        .filter_map(|b| match split_todo(&b.text) {
            (Some(s @ (TodoState::Todo | TodoState::Doing)), body) => Some((s, b, body)),
            _ => None,
        })
        .collect();
    // Journal slugs are ISO dates, so a descending slug puts the latest
    // day first. Named pages follow; nobody scans those by recency.
    open.sort_by(|(sa, a, _), (sb, b, _)| {
        let started = |s: &TodoState| u8::from(*s != TodoState::Doing);
        started(sa)
            .cmp(&started(sb))
            .then_with(|| b.source_slug.cmp(&a.source_slug))
            .then_with(|| a.source_block_path.cmp(&b.source_block_path))
    });
    let total = open.len();
    let tasks = open
        .into_iter()
        .take(limit)
        .map(|(s, b, body)| TaskHit {
            id: b.id.to_string(),
            source_slug: b.source_slug.clone(),
            status: if s == TodoState::Doing {
                "doing"
            } else {
                "todo"
            }
            .into(),
            text: body.replace('\n', " ").trim().to_string(),
        })
        .collect();
    OpenTasks { total, tasks }
}

#[cfg(test)]
mod tests {
    use super::*;
    use outl_core::id::NodeId;

    fn block(slug: &str, path: &[usize], text: &str) -> BlockEntry {
        BlockEntry {
            id: NodeId::new(),
            ref_handle: "blk-000000".into(),
            source_slug: slug.into(),
            source_path: format!("{slug}.md").into(),
            source_block_path: path.to_vec(),
            text: text.into(),
            text_fold: text.to_lowercase(),
            properties: Vec::new(),
            children: Vec::new(),
        }
    }

    #[test]
    fn keeps_open_tasks_and_orders_started_then_latest_day() {
        let blocks = [
            block("2026-10-10", &[0], "TODO older day"),
            block("2026-10-12", &[1], "TODO second on today"),
            block("2026-10-12", &[0], "[ ] first on today"),
            block("2026-10-11", &[0], "DONE finished"),
            block("2026-10-09", &[2], "DOING started a while ago"),
            block("inbox", &[0], "plain text, not a task"),
        ];
        let out = collect_open_tasks(blocks.iter(), 10);
        let texts: Vec<_> = out.tasks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "started a while ago",
                "first on today",
                "second on today",
                "older day",
            ]
        );
        assert_eq!(out.total, 4);
        assert_eq!(out.tasks[0].status, "doing");
        assert_eq!(out.tasks[1].status, "todo");
    }

    #[test]
    fn the_total_survives_the_cap() {
        let blocks: Vec<_> = (0..5).map(|i| block("inbox", &[i], "TODO x")).collect();
        let out = collect_open_tasks(blocks.iter(), 2);
        assert_eq!(out.tasks.len(), 2);
        assert_eq!(out.total, 5);
    }
}
