//! The `insert::` template anchor (issue #321).
//!
//! A structural template's root blocks nest **under** the invoked block by
//! default. A template page carrying `insert:: after` stamps them as
//! siblings at the invoked block's own level instead. The anchor is a
//! template-page property, resolved inside `outl-actions`, so every client
//! picks it up from the op log without a signature change.

use outl_actions::block::append_block;
use outl_actions::page::{open_or_create, set_property, PageKind};
use outl_actions::template::{instantiate_template, FROM_TEMPLATE_KEY, INSERT_KEY, TEMPLATE_KEY};
use outl_actions::tree::children_of;
use outl_core::hlc::HlcGenerator;
use outl_core::id::{ActorId, NodeId};
use outl_core::property::PropValue;
use outl_core::workspace::Workspace;

fn ws() -> (Workspace, HlcGenerator) {
    let actor = ActorId::new();
    (
        Workspace::open_in_memory(actor).unwrap(),
        HlcGenerator::new(actor),
    )
}

/// Build a template page and tag it with `template:: <name>`.
fn template_with(w: &mut Workspace, hlc: &HlcGenerator, slug: &str, name: &str) -> NodeId {
    let page = open_or_create(w, hlc, slug, name, PageKind::Page).unwrap();
    set_property(
        w,
        hlc,
        page,
        TEMPLATE_KEY,
        Some(PropValue::Text(name.into())),
    )
    .unwrap();
    page
}

fn set_insert(w: &mut Workspace, hlc: &HlcGenerator, page: NodeId, value: &str) {
    set_property(
        w,
        hlc,
        page,
        INSERT_KEY,
        Some(PropValue::Text(value.into())),
    )
    .unwrap();
}

fn texts(w: &Workspace, parent: NodeId) -> Vec<String> {
    children_of(w, parent)
        .into_iter()
        .filter_map(|(id, _)| w.block_text(id))
        .collect()
}

#[test]
fn insert_after_places_root_clones_as_siblings_of_the_target() {
    let (mut w, hlc) = ws();
    let tpl = template_with(&mut w, &hlc, "template-sibs", "sibs");
    let a = append_block(&mut w, &hlc, Some(tpl), Some("a")).unwrap();
    append_block(&mut w, &hlc, Some(a), Some("a1")).unwrap();
    append_block(&mut w, &hlc, Some(tpl), Some("b")).unwrap();
    set_insert(&mut w, &hlc, tpl, "after");

    // A page holding a host block that already has a child.
    let page = open_or_create(&mut w, &hlc, "p", "P", PageKind::Page).unwrap();
    let host = append_block(&mut w, &hlc, Some(page), Some("host")).unwrap();
    append_block(&mut w, &hlc, Some(host), Some("existing")).unwrap();

    let new_ids = instantiate_template(&mut w, &hlc, "sibs", host, "p", None).unwrap();
    assert_eq!(new_ids.len(), 2);

    // Root clones land beside `host`, in template order, not under it.
    assert_eq!(texts(&w, page), vec!["host", "a", "b"]);
    // The target's own children are untouched by an `after` insert.
    assert_eq!(texts(&w, host), vec!["existing"]);
    // A root clone's template children still nest under the clone.
    assert_eq!(texts(&w, new_ids[0]), vec!["a1"]);
}

#[test]
fn insert_after_chains_multiple_roots_in_order() {
    let (mut w, hlc) = ws();
    let tpl = template_with(&mut w, &hlc, "template-chain", "chain");
    for t in ["a", "b", "c"] {
        append_block(&mut w, &hlc, Some(tpl), Some(t)).unwrap();
    }
    set_insert(&mut w, &hlc, tpl, "after");

    let page = open_or_create(&mut w, &hlc, "p", "P", PageKind::Page).unwrap();
    let host = append_block(&mut w, &hlc, Some(page), Some("host")).unwrap();

    instantiate_template(&mut w, &hlc, "chain", host, "p", None).unwrap();

    assert_eq!(texts(&w, page), vec!["host", "a", "b", "c"]);
}

#[test]
fn insert_after_still_traces_and_substitutes() {
    let (mut w, hlc) = ws();
    let tpl = template_with(&mut w, &hlc, "template-aftrace", "aftrace");
    append_block(&mut w, &hlc, Some(tpl), Some("on {{page}}")).unwrap();
    set_insert(&mut w, &hlc, tpl, "after");

    let page = open_or_create(&mut w, &hlc, "p", "P", PageKind::Page).unwrap();
    let host = append_block(&mut w, &hlc, Some(page), Some("host")).unwrap();

    let new_ids = instantiate_template(&mut w, &hlc, "aftrace", host, "p", None).unwrap();
    let clone = new_ids[0];
    assert_eq!(w.block_text(clone).unwrap(), "on p");
    assert!(matches!(
        w.tree().property(clone, FROM_TEMPLATE_KEY),
        Some(PropValue::Text(s)) if s == "template-aftrace"
    ));
}

#[test]
fn insert_under_nests_like_the_default() {
    let (mut w, hlc) = ws();
    let tpl = template_with(&mut w, &hlc, "template-under", "under");
    append_block(&mut w, &hlc, Some(tpl), Some("a")).unwrap();
    set_insert(&mut w, &hlc, tpl, "under");

    let page = open_or_create(&mut w, &hlc, "p", "P", PageKind::Page).unwrap();
    let host = append_block(&mut w, &hlc, Some(page), Some("host")).unwrap();

    instantiate_template(&mut w, &hlc, "under", host, "p", None).unwrap();

    assert_eq!(texts(&w, host), vec!["a"]);
    assert_eq!(texts(&w, page), vec!["host"]);
}

#[test]
fn insert_after_on_a_page_target_degrades_to_append() {
    let (mut w, hlc) = ws();
    let tpl = template_with(&mut w, &hlc, "template-pagetarget", "pagetarget");
    append_block(&mut w, &hlc, Some(tpl), Some("a")).unwrap();
    append_block(&mut w, &hlc, Some(tpl), Some("b")).unwrap();
    set_insert(&mut w, &hlc, tpl, "after");

    // Target is the page node itself (CLI `template apply --page` with no
    // `--block`): a page has no block siblings, so the clones must nest
    // under the page, not be fabricated beside it.
    let page = open_or_create(&mut w, &hlc, "p", "P", PageKind::Page).unwrap();
    let root_children_before = children_of(&w, NodeId::root()).len();
    instantiate_template(&mut w, &hlc, "pagetarget", page, "p", None).unwrap();

    assert_eq!(texts(&w, page), vec!["a", "b"]);
    // No ownerless sibling block was fabricated beside the page: root
    // gained nothing.
    assert_eq!(children_of(&w, NodeId::root()).len(), root_children_before);
}

#[test]
fn unknown_insert_value_nests() {
    let (mut w, hlc) = ws();
    let tpl = template_with(&mut w, &hlc, "template-unknown", "unknown");
    append_block(&mut w, &hlc, Some(tpl), Some("a")).unwrap();
    set_insert(&mut w, &hlc, tpl, "sideways");

    let page = open_or_create(&mut w, &hlc, "p", "P", PageKind::Page).unwrap();
    let host = append_block(&mut w, &hlc, Some(page), Some("host")).unwrap();

    instantiate_template(&mut w, &hlc, "unknown", host, "p", None).unwrap();

    assert_eq!(texts(&w, host), vec!["a"]);
    assert_eq!(texts(&w, page), vec!["host"]);
}

#[test]
fn insert_property_is_never_copied_onto_clones() {
    let (mut w, hlc) = ws();
    let tpl = template_with(&mut w, &hlc, "template-nocopy", "nocopy");
    append_block(&mut w, &hlc, Some(tpl), Some("a")).unwrap();
    set_insert(&mut w, &hlc, tpl, "after");

    let page = open_or_create(&mut w, &hlc, "p", "P", PageKind::Page).unwrap();
    let host = append_block(&mut w, &hlc, Some(page), Some("host")).unwrap();

    let new_ids = instantiate_template(&mut w, &hlc, "nocopy", host, "p", None).unwrap();
    for id in new_ids {
        assert!(
            w.tree().property(id, INSERT_KEY).is_none(),
            "the anchor lives on the template page, never on an instance"
        );
    }
}
