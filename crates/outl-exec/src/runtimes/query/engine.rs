//! Execution engine — filter, sort and collect matching blocks.

use super::dsl::{
    Dir, Filter, KindFilter, PropFilter, Query, Sort, SortKey, StatusFilter, TagFilter,
    ISO_DATE_FMT,
};
use chrono::{Duration, NaiveDate};
use outl_md::block_index::BlockEntry;
use outl_md::index::WorkspaceIndex;

/// Task state of a block, as read off its text prefix.
///
/// **This mirrors `outl_actions::TodoState`, which is the owner of
/// the marker vocabulary.** It cannot be imported: `outl-actions`
/// depends on this crate (for `run_code_block`), so the arrow only
/// points one way. Adding a state there means adding it here in the
/// same change — the pair is convention, not a compiler check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Todo,
    Doing,
    Done,
}

impl Status {
    /// Lowercase wire form used in `QueryHit.status`.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Todo => "todo",
            Status::Doing => "doing",
            Status::Done => "done",
        }
    }
}

/// One query hit — the data we need to render an embed.
pub struct Hit {
    /// Stable block id — a plugin can address it later.
    pub id: String,
    /// Block ref handle (`blk-XXXXXX`) for embed rendering.
    pub handle: String,
    /// Slug of the page hosting the block.
    pub page_slug: String,
    /// Task state, or `None` when the block is not a task.
    pub status: Option<Status>,
    /// Block text with the task prefix stripped.
    pub text: String,
    /// Non-empty `key:: value` properties, lowercased, in file order.
    pub properties: Vec<(String, String)>,
    /// Block creation time in milliseconds, decoded from the block's
    /// ULID id — `None` for ids not minted from a wall clock
    /// ([`SortKey::Created`] explains why those park last).
    pub created_ms: Option<u64>,
}

/// Run `query` against `index`, returning all matching blocks.
pub fn run(index: &WorkspaceIndex, query: &Query) -> Vec<Hit> {
    let today = chrono::Local::now().date_naive();

    index
        .iter_blocks()
        .filter_map(|entry| {
            let (status, body) = split_todo(&entry.text);
            let page = index.by_slug(&entry.source_slug);

            for f in &query.filters {
                if !matches(f, entry, status, page.map(|p| p.is_journal), &today) {
                    return None;
                }
            }

            Some(Hit {
                id: entry.id.to_string(),
                handle: entry.ref_handle.clone(),
                page_slug: entry.source_slug.clone(),
                status,
                text: body.to_string(),
                properties: entry.properties.clone(),
                // Wall-clock ms from the id's ULID timestamp half;
                // `None` when the id derives from a slug instead of
                // the clock (page roots — see `sort_hits`).
                created_ms: created_ms(entry),
            })
        })
        .collect()
}

/// Sort hits by the given criteria, in priority order (last key first
/// so the first key dominates after stable sort).
///
/// Missing values — a block without the property, without a parseable
/// date, with a hash-derived id, on a non-journal page — sort last in
/// **both** directions: `:desc` must not smuggle the unknown rows to
/// the top, they are unknown, not maximal. The sort is stable, so
/// equal keys keep prior order.
pub fn sort_hits(hits: &mut [Hit], keys: &[Sort]) {
    for key in keys.iter().rev() {
        let dir = key.dir;
        match &key.key {
            SortKey::Page => hits.sort_by(|a, b| dir.apply(a.page_slug.cmp(&b.page_slug))),
            // Unfinished work first, in the order it moves through:
            // TODO, DOING, DONE. A non-task sorts with TODO, which
            // is where it sat before DOING existed.
            SortKey::Status => hits.sort_by(|a, b| {
                let rank = |s: Option<Status>| s.unwrap_or(Status::Todo);
                dir.apply(rank(a.status).cmp(&rank(b.status)))
            }),
            SortKey::Text => hits.sort_by(|a, b| dir.apply(a.text.cmp(&b.text))),
            SortKey::Prop(k) => hits.sort_by(|a, b| {
                cmp_nulls_last(
                    dir,
                    prop_date(&a.properties, k),
                    prop_date(&b.properties, k),
                )
            }),
            SortKey::Created => {
                hits.sort_by(|a, b| cmp_nulls_last(dir, a.created_ms, b.created_ms))
            }
            SortKey::JournalDate => hits.sort_by(|a, b| {
                cmp_nulls_last(
                    dir,
                    parse_journal_date(&a.page_slug),
                    parse_journal_date(&b.page_slug),
                )
            }),
        }
    }
}

/// Compare optional sort values, `None` ordered after `Some` in both
/// directions — only present values get direction-applied.
fn cmp_nulls_last<T: Ord>(dir: Dir, a: Option<T>, b: Option<T>) -> std::cmp::Ordering {
    match (a, b) {
        (None, None) => std::cmp::Ordering::Equal,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(a), Some(b)) => dir.apply(a.cmp(&b)),
    }
}

/// Creation time for a hit, read off the block's id.
///
/// `NodeId::created_ms` applies the plausible-clock window: page roots
/// (`from_slug`) and callable-result blocks (`from_seed`) carry a hash
/// where the timestamp belongs and decode to `None`, so they sort last
/// under `sort: created` rather than by a fake timestamp.
fn created_ms(entry: &BlockEntry) -> Option<u64> {
    entry.id.created_ms()
}

/// Parses the earliest valid ISO date among the `key` property values.
/// A block can carry the property several times (a second `due::`
/// line), so the earliest valid date wins rather than whichever line
/// happens to come last.
fn prop_date(properties: &[(String, String)], key: &str) -> Option<NaiveDate> {
    properties
        .iter()
        .filter(|(k, _)| k == key)
        .filter_map(|(_, v)| NaiveDate::parse_from_str(v, ISO_DATE_FMT).ok())
        .min()
}

fn matches(
    f: &Filter,
    entry: &BlockEntry,
    status: Option<Status>,
    is_journal: Option<bool>,
    today: &NaiveDate,
) -> bool {
    match f {
        Filter::Status(sf) => match sf {
            StatusFilter::Todo => status == Some(Status::Todo),
            StatusFilter::Doing => status == Some(Status::Doing),
            StatusFilter::Done => status == Some(Status::Done),
            // `open` has always meant "is a task", DONE included —
            // kept as-is so existing queries don't change meaning
            // under the user on an upgrade.
            StatusFilter::Open => status.is_some(),
        },
        Filter::Tag(t) => has_tag(entry, t),
        Filter::Prop(p) => has_prop(entry, p),
        Filter::Kind(kf) => match kf {
            KindFilter::Journal => is_journal == Some(true),
            KindFilter::Page => is_journal != Some(true),
        },
        // Date-property comparisons against the block's own ISO-date
        // properties. A block without the property, or with a value
        // that does not parse as a date, satisfies neither side.
        Filter::Before(k, d) => prop_date(&entry.properties, k).is_some_and(|pd| pd < *d),
        Filter::After(k, d) => prop_date(&entry.properties, k).is_some_and(|pd| pd > *d),
        // Scope by hosting page: the slug is precomputed per block,
        // so this is one string compare.
        Filter::Page(slug) => entry.source_slug == *slug,
        Filter::Since(days) => {
            is_journal == Some(true)
                && parse_journal_date(&entry.source_slug)
                    .map(|d| d >= *today - Duration::days(*days as i64))
                    .unwrap_or(false)
        }
        Filter::Text(needle) => entry.text_fold.contains(&needle.to_lowercase()),
        // Every `not-<key>` lands here. One `!` over the positive's
        // own arm is the whole implementation, which is what makes
        // `tag: x` plus `not-tag: x` return nothing: there is no
        // second matcher to disagree with the first.
        Filter::Not(inner) => !matches(inner, entry, status, is_journal, today),
    }
}

/// True when the block carries `#<name>` or a tag nested under it.
///
/// Routed through `outl_md::text_contains_tag_or_child` rather than a
/// `contains("#name")` on the cached fold: the substring form also
/// matches `#nameless`, which is harmless for `tag:` (an extra hit)
/// and silent data-hiding for `not-tag:` (a dropped one).
///
/// The fold is still the first gate. A boundary match is a subset of a
/// substring match, so a block whose text does not contain the needle
/// at all cannot match, and never reaches the tokenizer — which is
/// almost every block, on a filter that auto-runs on page load.
fn has_tag(entry: &BlockEntry, t: &TagFilter) -> bool {
    entry.text_fold.contains(t.needle())
        && outl_md::text_contains_tag_or_child(&entry.text, t.name())
}

/// True when the block carries property `key` — narrowed to one value
/// when the filter names one.
///
/// `BlockEntry::properties` is already lowercased, and so is
/// [`PropFilter`], so this compares folded against folded.
fn has_prop(entry: &BlockEntry, p: &PropFilter) -> bool {
    entry
        .properties
        .iter()
        .any(|(k, v)| k == &p.key && p.value.as_ref().is_none_or(|want| v == want))
}

/// Split a block's text into `(status, body)`, accepting both the
/// canonical word form (`"TODO body"`) and the CommonMark checkbox
/// form (`"[ ] body"`). Mirrors `outl_actions::split_todo` — see
/// [`Status`] for why it is a mirror and not a call.
///
/// The checkbox spellings have to be here too, or a block the user
/// wrote as `- [ ] ship it` renders a checkbox everywhere and then
/// fails to match `status: todo`, which is the same "it's a task
/// except where it isn't" split issue #230 was filed about.
fn split_todo(raw: &str) -> (Option<Status>, &str) {
    const PREFIXES: [(&str, Status); 7] = [
        ("TODO ", Status::Todo),
        ("DOING ", Status::Doing),
        ("DONE ", Status::Done),
        ("[ ] ", Status::Todo),
        ("[/] ", Status::Doing),
        ("[x] ", Status::Done),
        ("[X] ", Status::Done),
    ];
    // The marker may also sit after a single `"> "` quote prefix —
    // the legacy authoring shape (`"> TODO foo"`) that the TUI's
    // `split_block_prefixes` renders as a checkbox. The canonical
    // order (`"TODO > foo"`) already matches marker-first, and only
    // one quote marker is unwrapped, mirroring the "no nested
    // quotes" policy of `outl_actions::quote`.
    let after_quote = raw.strip_prefix("> ").unwrap_or(raw);
    for (prefix, status) in PREFIXES {
        if let Some(rest) = after_quote.strip_prefix(prefix) {
            return (Some(status), rest);
        }
    }
    (None, raw)
}

fn parse_journal_date(slug: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(slug, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests;
