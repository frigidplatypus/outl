//! Property model. Properties are key-value pairs attached to a node.
//!
//! Page-level properties live at the top of a `.md` file; block-level
//! properties are children of a block with `key:: value` syntax. Internally
//! both routes resolve to `SetProp` ops on the relevant node.

use serde::{Deserialize, Serialize};

/// Value types supported as property values.
///
/// The surface is intentionally narrow today; the query DSL may expand it later.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PropValue {
    /// Plain text value.
    Text(String),
    /// Reference to a page by name (e.g. `[[avelino]]`).
    PageRef(String),
    /// Tag reference (e.g. `#produto`).
    Tag(String),
    /// Multiple values (e.g. `tags:: #a #b`).
    List(Vec<PropValue>),
}

/// A property on a node — name + value.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Property {
    /// Property key (e.g. `priority`).
    pub key: String,
    /// Property value.
    pub value: PropValue,
}

impl PropValue {
    /// The value as a flat string — the **single owner** of the
    /// stringification rule every surface compares filters against.
    ///
    /// `Text` / `PageRef` / `Tag` yield their inner string verbatim;
    /// `List` joins its elements with `", "` (recursively). Any surface
    /// that answers "does this block carry `key:: value`?" (the plugin
    /// host's `prop` filter today, the query DSL's when it lands) must
    /// flatten through this method so two surfaces can never disagree
    /// about what a value *is* as a string.
    pub fn flatten(&self) -> String {
        match self {
            PropValue::Text(s) | PropValue::PageRef(s) | PropValue::Tag(s) => s.clone(),
            PropValue::List(items) => items
                .iter()
                .map(Self::flatten)
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_yields_the_inner_string_for_scalar_variants() {
        assert_eq!(PropValue::Text("16".into()).flatten(), "16");
        assert_eq!(PropValue::PageRef("avelino".into()).flatten(), "avelino");
        assert_eq!(PropValue::Tag("produto".into()).flatten(), "produto");
    }

    #[test]
    fn flatten_joins_list_elements_with_comma_space() {
        let list = PropValue::List(vec![PropValue::Tag("a".into()), PropValue::Tag("b".into())]);
        assert_eq!(list.flatten(), "a, b");
    }

    #[test]
    fn flatten_is_recursive_over_nested_lists() {
        let nested = PropValue::List(vec![
            PropValue::Text("x".into()),
            PropValue::List(vec![
                PropValue::Text("y".into()),
                PropValue::Text("z".into()),
            ]),
        ]);
        assert_eq!(nested.flatten(), "x, y, z");
    }

    #[test]
    fn flatten_of_an_empty_list_is_empty() {
        assert_eq!(PropValue::List(vec![]).flatten(), "");
    }
}
