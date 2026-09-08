//! `query` runtime — declarative workspace queries as code blocks.
//!
//! A ` ```query ` fence runs a line-by-line declarative DSL against the
//! workspace and returns matching blocks as **embed references**
//! (`!((blk-XXXXXX))`), not copies. This means toggling a TODO on the
//! original block is reflected everywhere the query result appears.
//!
//! Two entry points into the same engine:
//!
//! - **DSL string** (` ```query ` code block) — user-facing, renders embeds.
//! - **Structured API** (`run_query_structured`) — plugin-facing, returns
//!   typed `QueryHit` values. Exposed to JS as `outl.query({ … })`.
//!
//! Both converge on the same `Query` + `engine::run` pipeline.

use std::path::Path;
use std::time::Instant;

use outl_md::index::WorkspaceIndex;

use crate::runtime::{ExecContext, ExecError, ExecOutput, ExitStatus, OutputFormat, Runtime};

// ── Public query API (used by both ```query and plugin SDK) ─────────────

/// Structured query parameters — the plugin-facing API.
///
/// Every field is optional; an empty struct matches every block.
/// This is the shape that `outl.query({ … })` deserialises from JS.
#[derive(Debug, Default, Clone)]
pub struct QueryParams {
    /// `"todo"`, `"doing"`, `"done"`, or `"open"` (any task, DONE
    /// included).
    pub status: Option<String>,
    /// Partial tag match (without `#`).
    pub tag: Option<String>,
    /// Exclude blocks containing this tag.
    pub not_tag: Option<String>,
    /// Filter by block property (key, value).
    pub prop: Option<(String, String)>,
    /// Exclude blocks with this property key (any value) or key-value pair.
    pub not_prop: Option<(String, Option<String>)>,
    /// Filter by hosting page slug.
    pub page: Option<String>,
    /// `"journal"` or `"page"`.
    pub kind: Option<String>,
    /// Duration like `"7d"`, `"2w"`, `"3m"`.
    pub since: Option<String>,
    /// Substring search (case-insensitive).
    pub text: Option<String>,
    /// Sort keys in priority order.
    pub sort: Vec<String>,
    /// Maximum number of results.
    pub limit: Option<usize>,
}

/// One query result — structured, typed, no markdown.
#[derive(Debug, Clone)]
pub struct QueryHit {
    /// Block ref handle (`blk-XXXXXX`).
    pub handle: String,
    /// Slug of the hosting page.
    pub page: String,
    /// `"todo"`, `"doing"`, `"done"`, or `None` when the block is not
    /// a task.
    pub status: Option<String>,
    /// Block text with the task prefix stripped, in either spelling
    /// (`TODO ` / `[ ] `) and behind an optional `"> "` quote marker.
    pub text: String,
}

/// Run a query from structured parameters against the workspace at
/// `workspace_root`. Returns sorted, limited hits.
pub fn run_query_structured(
    params: &QueryParams,
    workspace_root: &Path,
) -> Result<Vec<QueryHit>, String> {
    let query = build_query_from_params(params)?;
    run_query_internal(&query, workspace_root)
}

/// Run a query from a DSL string against the workspace at
/// `workspace_root`. Returns sorted, limited hits.
pub fn run_query_dsl(dsl: &str, workspace_root: &Path) -> Result<Vec<QueryHit>, String> {
    let query = dsl::parse(dsl).map_err(|e| e.to_string())?;
    run_query_internal(&query, workspace_root)
}

/// Run `query` against an index the caller already holds.
///
/// This is the path every caller should be on: building a
/// `WorkspaceIndex` costs a walk of every `.md` plus every sidecar, and
/// a page with several ` ```query ` fences would otherwise pay it once
/// per fence.
pub fn run_query_dsl_with_index(
    dsl: &str,
    index: &WorkspaceIndex,
) -> Result<Vec<QueryHit>, String> {
    let query = dsl::parse(dsl).map_err(|e| e.to_string())?;
    Ok(run_against(&query, index))
}

fn run_query_internal(query: &dsl::Query, workspace_root: &Path) -> Result<Vec<QueryHit>, String> {
    let index = WorkspaceIndex::build(workspace_root);
    Ok(run_against(query, &index))
}

fn run_against(query: &dsl::Query, index: &WorkspaceIndex) -> Vec<QueryHit> {
    let mut hits = engine::run(index, query);
    engine::sort_hits(&mut hits, &query.sort);
    if let Some(limit) = query.limit {
        hits.truncate(limit);
    }
    hits.into_iter()
        .map(|h| QueryHit {
            handle: h.handle,
            page: h.page_slug,
            status: h.status.map(|s| s.as_str().to_string()),
            text: h.text,
        })
        .collect()
}

fn build_query_from_params(p: &QueryParams) -> Result<dsl::Query, String> {
    let mut filters = Vec::new();
    if let Some(s) = &p.status {
        filters.push(dsl::Filter::Status(match s.as_str() {
            "todo" => dsl::StatusFilter::Todo,
            "doing" => dsl::StatusFilter::Doing,
            "done" => dsl::StatusFilter::Done,
            "open" => dsl::StatusFilter::Open,
            other => {
                return Err(format!(
                    "invalid status '{other}' (use todo|doing|done|open)"
                ))
            }
        }));
    }
    if let Some(t) = &p.tag {
        filters.push(dsl::Filter::Tag(t.clone()));
    }
    if let Some(t) = &p.not_tag {
        filters.push(dsl::Filter::NotTag(t.clone()));
    }
    if let Some((k, v)) = &p.prop {
        filters.push(dsl::Filter::Prop(k.clone(), v.clone()));
    }
    if let Some((k, v)) = &p.not_prop {
        filters.push(dsl::Filter::NotProp(k.clone(), v.clone()));
    }
    if let Some(slug) = &p.page {
        filters.push(dsl::Filter::Page(slug.clone()));
    }
    if let Some(k) = &p.kind {
        filters.push(dsl::Filter::Kind(match k.as_str() {
            "journal" => dsl::KindFilter::Journal,
            "page" => dsl::KindFilter::Page,
            other => return Err(format!("invalid kind '{other}' (use journal|page)")),
        }));
    }
    if let Some(s) = &p.since {
        filters.push(dsl::Filter::Since(parse_duration_pub(s)?));
    }
    if let Some(t) = &p.text {
        filters.push(dsl::Filter::Text(t.clone()));
    }
    let mut sort = Vec::new();
    for s in &p.sort {
        sort.push(match s.as_str() {
            "page" => dsl::SortKey::Page,
            "status" => dsl::SortKey::Status,
            "text" => dsl::SortKey::Text,
            other => return Err(format!("invalid sort key '{other}' (use page|status|text)")),
        });
    }
    Ok(dsl::Query {
        filters,
        sort,
        limit: p.limit,
    })
}

fn parse_duration_pub(v: &str) -> Result<u32, String> {
    if v.is_empty() {
        return Err("since requires a duration like '7d', '2w', '3m'".into());
    }
    let (num_str, unit) = v.split_at(v.len() - 1);
    let n: u32 = num_str
        .parse()
        .map_err(|_| format!("since: invalid number in '{v}'"))?;
    match unit {
        "d" => Ok(n),
        "w" => Ok(n * 7),
        "m" => Ok(n * 30),
        _ => Err(format!("since: unknown unit '{unit}' (use d, w, or m)")),
    }
}

/// Query runtime — runs the DSL against the workspace on disk.
pub struct QueryRuntime;

impl Runtime for QueryRuntime {
    fn language(&self) -> &'static str {
        "query"
    }

    fn auto_run(&self) -> bool {
        true
    }

    fn needs_workspace_index(&self) -> bool {
        true
    }

    fn execute(&self, source: &str, ctx: &ExecContext<'_>) -> Result<ExecOutput, ExecError> {
        let start = Instant::now();

        // Prefer the caller's index. Falling back to a disk build keeps
        // every existing caller working, but it re-reads the whole
        // workspace per fence — see `ExecContext::index`.
        let hits = match &ctx.index {
            Some(index) => run_query_dsl_with_index(source, index),
            None => run_query_dsl(source, &ctx.workspace_root),
        }
        .map_err(ExecError::Language)?;

        let stdout = hits
            .iter()
            .map(|h| format!("!(({}))", h.handle))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ExecOutput {
            stdout,
            stderr: String::new(),
            duration: start.elapsed(),
            exit: ExitStatus::Ok,
            format: OutputFormat::Embeds,
        })
    }
}

/// DSL parser.
pub(crate) mod dsl {
    use std::fmt;

    /// Parsed query.
    #[derive(Debug, Default)]
    pub struct Query {
        pub filters: Vec<Filter>,
        pub sort: Vec<SortKey>,
        pub limit: Option<usize>,
    }

    #[derive(Debug, Clone)]
    pub enum Filter {
        Status(StatusFilter),
        Tag(String),
        /// Exclude blocks containing this tag.
        NotTag(String),
        /// Filter by block property (key, value).
        Prop(String, String),
        /// Exclude blocks with this property key (any value) or key-value pair.
        NotProp(String, Option<String>),
        /// Filter by hosting page slug.
        Page(String),
        Kind(KindFilter),
        Since(u32),
        Text(String),
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum StatusFilter {
        Todo,
        Doing,
        Done,
        Open,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum KindFilter {
        Journal,
        Page,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum SortKey {
        Page,
        Status,
        Text,
    }

    #[derive(Debug)]
    pub struct ParseError {
        /// 1-based line number where the error occurred.
        pub line: usize,
        /// Human-readable description.
        pub msg: String,
    }

    impl fmt::Display for ParseError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "line {}: {}", self.line, self.msg)
        }
    }

    /// Parse a query DSL source string into a [`Query`].
    pub fn parse(source: &str) -> Result<Query, ParseError> {
        let mut filters = Vec::new();
        let mut sort = Vec::new();
        let mut limit = None;

        for (i, raw) in source.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // `prop` and `not-prop` take the rest of the line as their
            // value (which itself contains a key-value pair), so they
            // need special handling before the generic split_kv.
            // Word-boundary check: `prop` must be followed by `:`,
            // space, or end-of-line — otherwise `property: foo`
            // silently becomes `prop erty: foo`.
            if line == "prop" || line.starts_with("prop ") || line.starts_with("prop:") {
                let rest = line[4..].trim();
                let rest = rest.strip_prefix(':').map(|s| s.trim()).unwrap_or(rest);
                if rest.is_empty() {
                    return Err(ParseError {
                        line: i + 1,
                        msg: "prop requires 'key: value' or 'key value'".into(),
                    });
                }
                let (pk, pv) = parse_prop(rest, i)?;
                filters.push(Filter::Prop(pk, pv));
                continue;
            }
            if line == "not-prop" || line.starts_with("not-prop ") || line.starts_with("not-prop:") {
                let rest = line[8..].trim();
                let rest = rest.strip_prefix(':').map(|s| s.trim()).unwrap_or(rest);
                if rest.is_empty() {
                    return Err(ParseError {
                        line: i + 1,
                        msg: "not-prop requires a key".into(),
                    });
                }
                let (pk, pv) = parse_prop_optional_value(rest, i)?;
                filters.push(Filter::NotProp(pk, pv));
                continue;
            }

            let (key, value) = split_kv(line, i)?;
            let key = key.trim();
            let value = value.trim();

            match key {
                "status" => filters.push(Filter::Status(parse_status(value, i)?)),
                "tag" => filters.push(Filter::Tag(value.to_string())),
                "not-tag" => filters.push(Filter::NotTag(value.to_string())),
                "page" => filters.push(Filter::Page(value.to_string())),
                "kind" => filters.push(Filter::Kind(parse_kind(value, i)?)),
                "since" => filters.push(Filter::Since(parse_duration(value, i)?)),
                "text" => filters.push(Filter::Text(value.to_string())),
                "sort" => {
                    for part in value.split(',') {
                        sort.push(parse_sort_key(part.trim(), i)?);
                    }
                }
                "limit" => {
                    limit = Some(parse_usize(value, i)?);
                }
                _ => {
                    return Err(ParseError {
                        line: i + 1,
                        msg: format!("unknown key: '{key}'"),
                    });
                }
            }
        }

        Ok(Query {
            filters,
            sort,
            limit,
        })
    }

    fn split_kv(line: &str, line_idx: usize) -> Result<(&str, &str), ParseError> {
        line.split_once(':').ok_or_else(|| ParseError {
            line: line_idx + 1,
            msg: "expected 'key: value'".into(),
        })
    }

    fn parse_status(v: &str, line_idx: usize) -> Result<StatusFilter, ParseError> {
        match v {
            "todo" => Ok(StatusFilter::Todo),
            "doing" => Ok(StatusFilter::Doing),
            "done" => Ok(StatusFilter::Done),
            "open" => Ok(StatusFilter::Open),
            _ => Err(ParseError {
                line: line_idx + 1,
                msg: format!("status must be 'todo', 'doing', 'done', or 'open', got '{v}'"),
            }),
        }
    }

    fn parse_kind(v: &str, line_idx: usize) -> Result<KindFilter, ParseError> {
        match v {
            "journal" => Ok(KindFilter::Journal),
            "page" => Ok(KindFilter::Page),
            _ => Err(ParseError {
                line: line_idx + 1,
                msg: format!("kind must be 'journal' or 'page', got '{v}'"),
            }),
        }
    }

    /// Parse `Nd` / `Nw` / `Nm` into a day count.
    fn parse_duration(v: &str, line_idx: usize) -> Result<u32, ParseError> {
        if v.is_empty() {
            return Err(ParseError {
                line: line_idx + 1,
                msg: "since requires a duration like '7d', '2w', '3m'".into(),
            });
        }
        let (num_str, unit) = v.split_at(v.len() - 1);
        let n: u32 = num_str.parse().map_err(|_| ParseError {
            line: line_idx + 1,
            msg: format!("since: invalid number in '{v}'"),
        })?;
        match unit {
            "d" => Ok(n),
            "w" => Ok(n * 7),
            "m" => Ok(n * 30),
            _ => Err(ParseError {
                line: line_idx + 1,
                msg: format!("since: unknown unit '{unit}' (use d, w, or m)"),
            }),
        }
    }

    fn parse_sort_key(v: &str, line_idx: usize) -> Result<SortKey, ParseError> {
        match v {
            "page" => Ok(SortKey::Page),
            "status" => Ok(SortKey::Status),
            "text" => Ok(SortKey::Text),
            _ => Err(ParseError {
                line: line_idx + 1,
                msg: format!("sort: must be 'page', 'status', or 'text', got '{v}'"),
            }),
        }
    }

    fn parse_usize(v: &str, line_idx: usize) -> Result<usize, ParseError> {
        v.parse::<usize>().map_err(|_| ParseError {
            line: line_idx + 1,
            msg: format!("expected a number, got '{v}'"),
        })
    }

    /// Parse `key: value` or `key value` from a prop directive value.
    fn parse_prop(v: &str, line_idx: usize) -> Result<(String, String), ParseError> {
        // Accept both `prop priority: high` and `prop priority high`
        let (key, value) = if let Some((k, val)) = v.split_once(':') {
            (k.trim(), val.trim())
        } else if let Some((k, val)) = v.split_once(' ') {
            (k.trim(), val.trim())
        } else {
            return Err(ParseError {
                line: line_idx + 1,
                msg: format!("prop requires 'key: value' or 'key value', got '{v}'"),
            });
        };
        if key.is_empty() || value.is_empty() {
            return Err(ParseError {
                line: line_idx + 1,
                msg: format!("prop requires non-empty key and value, got '{v}'"),
            });
        }
        Ok((key.to_string(), value.to_string()))
    }

    /// Parse `key` or `key: value` from a not-prop directive value.
    /// Value is optional — `not-prop: status` excludes any block with `status::`.
    fn parse_prop_optional_value(
        v: &str,
        line_idx: usize,
    ) -> Result<(String, Option<String>), ParseError> {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            return Err(ParseError {
                line: line_idx + 1,
                msg: "not-prop requires a key".into(),
            });
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let k = key.trim();
            let val = value.trim();
            if k.is_empty() {
                return Err(ParseError {
                    line: line_idx + 1,
                    msg: format!("not-prop requires a key, got '{v}'"),
                });
            }
            Ok((k.to_string(), if val.is_empty() { None } else { Some(val.to_string()) }))
        } else if let Some((key, value)) = trimmed.split_once(' ') {
            let k = key.trim();
            let val = value.trim();
            if k.is_empty() {
                return Err(ParseError {
                    line: line_idx + 1,
                    msg: format!("not-prop requires a key, got '{v}'"),
                });
            }
            Ok((k.to_string(), if val.is_empty() { None } else { Some(val.to_string()) }))
        } else {
            // Just a key, no value — exclude any block with that property
            Ok((trimmed.to_string(), None))
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_status_todo() {
            let q = parse("status: todo").unwrap();
            assert_eq!(q.filters.len(), 1);
            assert!(matches!(q.filters[0], Filter::Status(StatusFilter::Todo)));
        }

        #[test]
        fn parses_multiple_filters() {
            let q = parse("status: todo\ntag: ops\nlimit: 10").unwrap();
            assert_eq!(q.filters.len(), 2);
            assert_eq!(q.limit, Some(10));
        }

        #[test]
        fn ignores_comments() {
            let q = parse("# comment\nstatus: done").unwrap();
            assert_eq!(q.filters.len(), 1);
        }

        #[test]
        fn parses_sort() {
            let q = parse("sort: page, status").unwrap();
            assert_eq!(q.sort.len(), 2);
        }

        #[test]
        fn parses_since() {
            let q = parse("since: 2w").unwrap();
            assert!(matches!(q.filters[0], Filter::Since(14)));
        }

        #[test]
        fn rejects_unknown_key() {
            assert!(parse("bogus: value").is_err());
        }

        #[test]
        fn parses_not_tag() {
            let q = parse("not-tag: western").unwrap();
            assert_eq!(q.filters.len(), 1);
            assert!(matches!(&q.filters[0], Filter::NotTag(t) if t == "western"));
        }

        #[test]
        fn parses_prop_with_colon() {
            let q = parse("prop priority: high").unwrap();
            assert_eq!(q.filters.len(), 1);
            assert!(matches!(&q.filters[0], Filter::Prop(k, v) if k == "priority" && v == "high"));
        }

        #[test]
        fn parses_prop_with_space() {
            let q = parse("prop priority high").unwrap();
            assert_eq!(q.filters.len(), 1);
            assert!(matches!(&q.filters[0], Filter::Prop(k, v) if k == "priority" && v == "high"));
        }

        #[test]
        fn parses_not_prop_key_only() {
            let q = parse("not-prop: status").unwrap();
            assert_eq!(q.filters.len(), 1);
            assert!(matches!(&q.filters[0], Filter::NotProp(k, None) if k == "status"));
        }

        #[test]
        fn parses_not_prop_key_and_value() {
            let q = parse("not-prop: status: done").unwrap();
            assert_eq!(q.filters.len(), 1);
            assert!(matches!(&q.filters[0], Filter::NotProp(k, Some(v)) if k == "status" && v == "done"));
        }

        #[test]
        fn parses_page_filter() {
            let q = parse("page: inbox").unwrap();
            assert_eq!(q.filters.len(), 1);
            assert!(matches!(&q.filters[0], Filter::Page(s) if s == "inbox"));
        }

        #[test]
        fn prop_requires_value() {
            assert!(parse("prop priority").is_err());
        }

        #[test]
        fn property_is_not_prop() {
            // `property: foo` must not silently parse as `prop erty: foo`.
            assert!(parse("property: foo").is_err());
        }

        #[test]
        fn not_property_is_not_not_prop() {
            // `not-property: foo` must not silently parse as `not-prop erty: foo`.
            assert!(parse("not-property: foo").is_err());
        }

        #[test]
        fn not_tag_combined_with_tag() {
            let q = parse("tag: ops\nnot-tag: western").unwrap();
            assert_eq!(q.filters.len(), 2);
        }
    }
}

/// Execution engine — filter + collect matching blocks.
pub(crate) mod engine {
    use super::dsl::{Filter, KindFilter, Query, SortKey, StatusFilter};
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
        /// Block ref handle (`blk-XXXXXX`) for embed rendering.
        pub handle: String,
        /// Slug of the page hosting the block.
        pub page_slug: String,
        /// Task state, or `None` when the block is not a task.
        pub status: Option<Status>,
        /// Block text with the task prefix stripped.
        pub text: String,
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
                    handle: entry.ref_handle.clone(),
                    page_slug: entry.source_slug.clone(),
                    status,
                    text: body.to_string(),
                })
            })
            .collect()
    }

    /// Sort hits by the given criteria, in priority order (last key first
    /// so the first key dominates after stable sort).
    pub fn sort_hits(hits: &mut [Hit], keys: &[SortKey]) {
        for key in keys.iter().rev() {
            match key {
                SortKey::Page => hits.sort_by(|a, b| a.page_slug.cmp(&b.page_slug)),
                // Unfinished work first, in the order it moves through:
                // TODO, DOING, DONE. A non-task sorts with TODO, which
                // is where it sat before DOING existed.
                SortKey::Status => hits.sort_by(|a, b| {
                    let rank = |s: Option<Status>| s.unwrap_or(Status::Todo);
                    rank(a.status).cmp(&rank(b.status))
                }),
                SortKey::Text => hits.sort_by(|a, b| a.text.cmp(&b.text)),
            }
        }
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
            Filter::Tag(tag) => {
                let needle = format!("#{}", tag.to_lowercase());
                entry.text_fold.contains(&needle)
            }
            Filter::NotTag(tag) => {
                let needle = format!("#{}", tag.to_lowercase());
                !entry.text_fold.contains(&needle)
            }
            Filter::Prop(key, value) => {
                let key_fold = key.to_lowercase();
                let value_fold = value.to_lowercase();
                entry.properties.iter().any(|(k, v)| {
                    k.to_lowercase() == key_fold
                        && v.to_lowercase() == value_fold
                })
            }
            Filter::NotProp(key, value) => {
                let key_fold = key.to_lowercase();
                if let Some(val) = value {
                    let value_fold = val.to_lowercase();
                    !entry.properties.iter().any(|(k, v)| {
                        k.to_lowercase() == key_fold
                            && v.to_lowercase() == value_fold
                    })
                } else {
                    // No value specified — exclude any block with this key
                    !entry.properties.iter().any(|(k, _)| {
                        k.to_lowercase() == key_fold
                    })
                }
            }
            Filter::Page(slug) => entry.source_slug == *slug,
            Filter::Kind(kf) => match kf {
                KindFilter::Journal => is_journal == Some(true),
                KindFilter::Page => is_journal != Some(true),
            },
            Filter::Since(days) => {
                is_journal == Some(true)
                    && parse_journal_date(&entry.source_slug)
                        .map(|d| d >= *today - Duration::days(*days as i64))
                        .unwrap_or(false)
            }
            Filter::Text(needle) => entry.text_fold.contains(&needle.to_lowercase()),
        }
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
    mod tests {
        use super::*;

        #[test]
        fn split_todo_open() {
            assert_eq!(
                split_todo("TODO buy milk"),
                (Some(Status::Todo), "buy milk")
            );
        }

        #[test]
        fn split_todo_doing() {
            assert_eq!(
                split_todo("DOING buy milk"),
                (Some(Status::Doing), "buy milk")
            );
        }

        #[test]
        fn split_todo_done() {
            assert_eq!(
                split_todo("DONE buy milk"),
                (Some(Status::Done), "buy milk")
            );
        }

        #[test]
        fn split_todo_none() {
            assert_eq!(split_todo("just text"), (None, "just text"));
        }

        #[test]
        fn split_todo_sees_a_marker_behind_a_quote_marker() {
            // Legacy authoring order. The TUI draws it as a checkbox,
            // so `status: todo` has to find it too.
            assert_eq!(
                split_todo("> TODO buy milk"),
                (Some(Status::Todo), "buy milk")
            );
            assert_eq!(
                split_todo("> [ ] buy milk"),
                (Some(Status::Todo), "buy milk")
            );
            // Canonical order still works, quote and all.
            assert_eq!(
                split_todo("TODO > buy milk"),
                (Some(Status::Todo), "> buy milk")
            );
            // One quote marker only, matching `outl_actions::quote`.
            assert_eq!(split_todo("> > TODO x"), (None, "> > TODO x"));
            // A plain quote is not a task.
            assert_eq!(split_todo("> just a quote"), (None, "> just a quote"));
        }

        #[test]
        fn split_todo_reads_the_checkbox_spelling() {
            // A block the user typed as `- [ ] ship it` has to match
            // `status: todo`, or it renders a checkbox everywhere and
            // then goes missing from the query (issue #230).
            assert_eq!(split_todo("[ ] buy milk"), (Some(Status::Todo), "buy milk"));
            assert_eq!(
                split_todo("[/] buy milk"),
                (Some(Status::Doing), "buy milk")
            );
            assert_eq!(split_todo("[x] buy milk"), (Some(Status::Done), "buy milk"));
            // A link is not a checkbox.
            assert_eq!(
                split_todo("[x](https://example.com)"),
                (None, "[x](https://example.com)")
            );
        }

        #[test]
        fn split_todo_reads_the_quote_first_shape() {
            // `"> TODO foo"` is the legacy authoring order the TUI
            // renders as a quoted checkbox, so a `status:` filter has
            // to see it too — a task on one surface is a task on all
            // of them.
            assert_eq!(
                split_todo("> TODO buy milk"),
                (Some(Status::Todo), "buy milk")
            );
            assert_eq!(
                split_todo("> [ ] buy milk"),
                (Some(Status::Todo), "buy milk")
            );
            assert_eq!(
                split_todo("> [x] buy milk"),
                (Some(Status::Done), "buy milk")
            );
            // A plain quote is not a task, and only one quote marker
            // is unwrapped (no nested quotes).
            assert_eq!(split_todo("> just a quote"), (None, "> just a quote"));
            assert_eq!(split_todo("> > TODO foo"), (None, "> > TODO foo"));
        }

        #[test]
        fn doing_is_neither_todo_nor_done_to_a_filter() {
            // The whole point of the state: a query for open work must
            // not sweep up started work, and `status: done` must not
            // count something nobody finished.
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let text = "DOING ship the parser";
            let entry = BlockEntry {
                id: outl_core::NodeId::new(),
                ref_handle: "blk-aaaaaa".into(),
                source_slug: "notes".into(),
                source_path: std::path::PathBuf::from("pages/notes.md"),
                source_block_path: vec![0],
                text: text.into(),
                text_fold: text.to_lowercase(),
                properties: Vec::new(),
                children: Vec::new(),
            };
            let (status, _) = split_todo(&entry.text);
            for (filter, expected) in [
                (StatusFilter::Todo, false),
                (StatusFilter::Doing, true),
                (StatusFilter::Done, false),
                (StatusFilter::Open, true),
            ] {
                assert_eq!(
                    matches(&Filter::Status(filter), &entry, status, None, &today),
                    expected,
                    "{filter:?} against a DOING block"
                );
            }
        }

        fn make_entry(
            text: &str,
            slug: &str,
            props: Vec<(&str, &str)>,
        ) -> BlockEntry {
            BlockEntry {
                id: outl_core::NodeId::new(),
                ref_handle: "blk-test".into(),
                source_slug: slug.into(),
                source_path: std::path::PathBuf::from(format!("pages/{slug}.md")),
                source_block_path: vec![0],
                text: text.into(),
                text_fold: text.to_lowercase(),
                properties: props
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect(),
                children: Vec::new(),
            }
        }

        #[test]
        fn not_tag_excludes_matching_blocks() {
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let with_tag = make_entry("TODO thing #western", "notes", vec![]);
            let without_tag = make_entry("TODO thing #eastern", "notes", vec![]);
            let filter = Filter::NotTag("western".into());
            assert!(!matches(&filter, &with_tag, None, None, &today));
            assert!(matches(&filter, &without_tag, None, None, &today));
        }

        #[test]
        fn prop_filter_matches_key_value() {
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let entry = make_entry("TODO thing", "notes", vec![("priority", "high")]);
            let filter = Filter::Prop("priority".into(), "high".into());
            assert!(matches(&filter, &entry, None, None, &today));
            let wrong_val = Filter::Prop("priority".into(), "low".into());
            assert!(!matches(&wrong_val, &entry, None, None, &today));
        }

        #[test]
        fn prop_filter_is_case_insensitive() {
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let entry = make_entry("TODO thing", "notes", vec![("Priority", "HIGH")]);
            let filter = Filter::Prop("priority".into(), "high".into());
            assert!(matches(&filter, &entry, None, None, &today));
        }

        #[test]
        fn not_prop_key_only_excludes_any_block_with_that_key() {
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let with_prop = make_entry("TODO thing", "notes", vec![("status", "done")]);
            let without_prop = make_entry("TODO thing", "notes", vec![]);
            let filter = Filter::NotProp("status".into(), None);
            assert!(!matches(&filter, &with_prop, None, None, &today));
            assert!(matches(&filter, &without_prop, None, None, &today));
        }

        #[test]
        fn not_prop_with_value_excludes_only_that_pair() {
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let entry = make_entry("TODO thing", "notes", vec![("status", "done")]);
            let filter = Filter::NotProp("status".into(), Some("done".into()));
            assert!(!matches(&filter, &entry, None, None, &today));
            let other_val = Filter::NotProp("status".into(), Some("todo".into()));
            assert!(matches(&other_val, &entry, None, None, &today));
        }

        #[test]
        fn page_filter_matches_slug() {
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let entry = make_entry("TODO thing", "inbox", vec![]);
            let filter = Filter::Page("inbox".into());
            assert!(matches(&filter, &entry, None, None, &today));
            let wrong = Filter::Page("archive".into());
            assert!(!matches(&wrong, &entry, None, None, &today));
        }

        #[test]
        fn combined_tag_and_not_tag() {
            let today = chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
            let entry = make_entry("TODO thing #ops #western", "notes", vec![]);
            let tag_filter = Filter::Tag("ops".into());
            let not_tag_filter = Filter::NotTag("western".into());
            assert!(matches(&tag_filter, &entry, None, None, &today));
            assert!(!matches(&not_tag_filter, &entry, None, None, &today));
        }
    }
}
