//! Template engine — structural templates + callable code blocks.
//!
//! A **template** is any page with a non-empty `template::` property.
//! The property value is the invocation name (what the user types
//! after `/template`). The page's outline is the template body.
//!
//! Two invocation modes:
//!
//! - **Structural** (`/template <name>`): deep-copy the template's
//!   subtree under the target block with built-in variable
//!   substitution. See [`instantiate::instantiate_template`].
//! - **Callable** (` ```call:<name> `): resolve the template's code
//!   block for execution with params. See [`call::resolve_call`].
//!
//! Traceability: structural instances get `from-template:: <slug>` on
//! each root block, and callable sites carry a ` ```call:<name> `
//! fence. Neither is a plain `[[ref]]` in the block text, so
//! [`crate::backlinks::backlinks_for_page`] recognizes both explicitly
//! when the target page is a template — that's how the template page's
//! backlinks panel surfaces every place it was rendered or instantiated.

/// Property key marking a page as a template.
pub const TEMPLATE_KEY: &str = "template";

/// Property key on instantiated blocks recording which template
/// they were created from.
pub const FROM_TEMPLATE_KEY: &str = "from-template";

/// Property key declaring a callable template's parameter names
/// (comma-separated).
pub const PARAMS_KEY: &str = "params";

/// Property key on a structural template page declaring where its
/// root blocks land relative to the block the template is invoked on:
/// `insert:: under` nests them as children (the default), `insert::
/// after` stamps them as siblings at the invoked block's own level.
/// Resolved by `resolve_anchor`; the value lives on the page node so
/// it reaches the op log as an ordinary `Op::SetProp` and never gets
/// copied onto an instance.
pub const INSERT_KEY: &str = "insert";

/// Where a structural template's root blocks land, read from the
/// template page's [`INSERT_KEY`] property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TemplateAnchor {
    /// Nest root clones as children of the invoked block (default).
    Under,
    /// Place root clones as siblings, immediately after the invoked
    /// block, at its own level.
    After,
}

/// Read a template page's [`INSERT_KEY`] to decide where its root
/// clones land. Permissive by design: only the literal `after` opts
/// into sibling placement, so an absent or empty property (and every
/// other value) keeps the historical nesting-under behaviour. An
/// unrecognised non-empty value warns once rather than silently
/// changing the shape of the insert.
pub(crate) fn resolve_anchor(
    workspace: &outl_core::workspace::Workspace,
    template_page: outl_core::id::NodeId,
) -> TemplateAnchor {
    match crate::page::read_text_prop(workspace, template_page, INSERT_KEY)
        .map(|v| v.trim().to_ascii_lowercase())
    {
        Some(v) if v == "after" => TemplateAnchor::After,
        Some(v) if !v.is_empty() && v != "under" => {
            tracing::warn!(
                value = %v,
                "unrecognised `insert::` template property; nesting as children"
            );
            TemplateAnchor::Under
        }
        _ => TemplateAnchor::Under,
    }
}

/// A node that cannot have meaningful siblings in the page model:
/// the tree root, or a page node (a page's siblings are other pages,
/// not ordinary blocks). A template asking for [`TemplateAnchor::After`]
/// degrades to nesting under such a target rather than fabricating a
/// block that belongs to no page.
pub(crate) fn is_page_or_root(
    workspace: &outl_core::workspace::Workspace,
    node: outl_core::id::NodeId,
) -> bool {
    node == outl_core::id::NodeId::root()
        || workspace
            .tree()
            .property(node, crate::page::SLUG_KEY)
            .is_some()
}

/// Reserved template name for the daily journal body. A page with
/// `template:: journal` is stamped into a fresh daily note
/// automatically the first time it is opened (see
/// [`crate::page::open_journal`]).
pub const JOURNAL_TEMPLATE_NAME: &str = "journal";

/// Parse a comma-separated `params::` property value into a list of
/// trimmed, non-empty parameter names.
pub(crate) fn parse_param_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

pub mod call;
pub mod instantiate;
pub mod list;
pub mod run;
pub mod vars;

pub use call::{
    call_target_name, inject_call_params, parse_call_params, resolve_call, CallResolution,
};
pub use instantiate::instantiate_template;
pub use list::{list_templates, TemplateEntry};
pub use run::{parse_call_invocation, run_callable_block};
