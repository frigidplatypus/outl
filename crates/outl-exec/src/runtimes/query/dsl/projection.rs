//! Output projection: what the `liq` clause grammar adds on top of the
//! flat DSL's filters and sorts.
//!
//! The flat ` ```query ` DSL never populates these — `Query::default`
//! carries the embed-list output it has always produced. The types
//! live in `dsl` (not beside `LiqRuntime`) because `Query` owns a
//! `Projection` field and the engine must understand it regardless of
//! which runtime fills it.

use super::SortKey;

/// One `select` column: a fixed field or a `prop:<key>` property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Column {
    /// The block's text as a live embed reference (`!((blk-…))`), so
    /// edits and TODO state show through between page loads.
    Text,
    /// The block reference in collapsed form (`((blk-…))`).
    Block,
    /// The hosting page as a wikilink (`[[slug|Title]]`).
    Page,
    /// The TODO state as a checkbox glyph (`[ ]` / `[/]` / `[x]`);
    /// empty for non-task blocks.
    Status,
    /// The block's tags, `#a #b`, space-separated.
    Tags,
    /// `journal` or `page`.
    Kind,
    /// A property value, joined with `, ` when the key repeats.
    Prop(String),
}

impl Column {
    /// Parses one `select` / column name: the fixed vocabulary, else a
    /// `prop:`-prefixed key, else a bare property key (the same
    /// fallback `parse_sort_str` uses for `sort:`).
    pub fn parse(name: &str) -> Option<Column> {
        let name = name.trim();
        let key = match name.strip_prefix("prop:") {
            Some(key) => key,
            None => {
                return Some(match name {
                    "text" => Column::Text,
                    "block" => Column::Block,
                    "page" => Column::Page,
                    "status" => Column::Status,
                    "tags" => Column::Tags,
                    "kind" => Column::Kind,
                    _ => Column::Prop(name.to_lowercase()),
                });
            }
        };
        if key.is_empty() {
            return None;
        }
        Some(Column::Prop(key.to_lowercase()))
    }

    /// The table header label for this column.
    pub fn header(&self) -> String {
        match self {
            Column::Text => "text".into(),
            Column::Block => "block".into(),
            Column::Page => "page".into(),
            Column::Status => "status".into(),
            Column::Tags => "tags".into(),
            Column::Kind => "kind".into(),
            Column::Prop(key) => format!("prop:{key}"),
        }
    }
}

/// How results render underneath the result block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Render {
    /// One `!((blk-…))` embed bullet per hit — what the `query`
    /// runtime has always produced.
    #[default]
    Embeds,
    /// One GFM pipe table with the `select` columns as headers.
    Table,
}

/// What and how to print: columns, grouping key, render mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Projection {
    pub select: Vec<Column>,
    pub group: Option<SortKey>,
    pub render: Render,
}
