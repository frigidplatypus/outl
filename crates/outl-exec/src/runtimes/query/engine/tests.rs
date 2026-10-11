//! Unit tests for the query engine's filter and sort behaviour.
//!
//! Split out of `engine.rs` for the file-size ratchet, the same way
//! `outline/tests.rs` was: the seam is the `#[cfg(test)]` boundary,
//! never a line inside the matcher itself.

use super::*;

/// A standalone indexed block, so filter behaviour can be asserted
/// without a workspace on disk.
fn entry(text: &str, properties: &[(&str, &str)]) -> BlockEntry {
    BlockEntry {
        id: outl_core::NodeId::new(),
        ref_handle: "blk-aaaaaa".into(),
        source_slug: "notes".into(),
        source_path: std::path::PathBuf::from("pages/notes.md"),
        source_block_path: vec![0],
        text: text.into(),
        text_fold: text.to_lowercase(),
        // The index folds these on the way in; mirror that here so
        // the test exercises the same comparison production does.
        properties: properties
            .iter()
            .map(|(k, v)| (k.to_lowercase(), v.to_lowercase()))
            .collect(),
        children: Vec::new(),
    }
}

fn hits(e: &BlockEntry, f: &Filter) -> bool {
    let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let (status, _) = split_todo(&e.text);
    matches(f, e, status, None, &today)
}

fn tag(name: &str) -> TagFilter {
    TagFilter::new(name).expect("a non-empty tag name")
}

fn prop(raw: &str) -> PropFilter {
    PropFilter::new(raw).expect("a well-formed prop filter")
}

/// What every `not-<key>` directive parses to.
fn not(f: Filter) -> Filter {
    Filter::Not(Box::new(f))
}

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
    assert_eq!(
        split_todo("> [x] buy milk"),
        (Some(Status::Done), "buy milk")
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
fn doing_is_neither_todo_nor_done_to_a_filter() {
    // The whole point of the state: a query for open work must
    // not sweep up started work, and `status: done` must not
    // count something nobody finished.
    let e = entry("DOING ship the parser", &[]);
    for (filter, expected) in [
        (StatusFilter::Todo, false),
        (StatusFilter::Doing, true),
        (StatusFilter::Done, false),
        (StatusFilter::Open, true),
    ] {
        assert_eq!(
            hits(&e, &Filter::Status(filter)),
            expected,
            "{filter:?} against a DOING block"
        );
    }
}

#[test]
fn not_tag_drops_the_block_carrying_the_tag() {
    let e = entry("TODO read the paper #research", &[]);
    assert!(hits(&e, &Filter::Tag(tag("research"))));
    assert!(!hits(&e, &not(Filter::Tag(tag("research")))));
}

#[test]
fn not_tag_keeps_a_block_without_the_tag() {
    let e = entry("TODO ship the parser #work", &[]);
    assert!(hits(&e, &not(Filter::Tag(tag("research")))));
}

#[test]
fn not_tag_stops_at_the_tag_boundary() {
    // Issue #323's open question 1. A substring negative would
    // drop this block for `not-tag: work`, hiding live work
    // behind a filter the user wrote to hide something else.
    let e = entry("TODO document the #workflow", &[]);
    assert!(hits(&e, &not(Filter::Tag(tag("work")))));
    assert!(!hits(&e, &Filter::Tag(tag("work"))));
}

#[test]
fn a_tag_filter_still_answers_for_its_namespace_children() {
    // Documented behaviour of `tag:` since it shipped —
    // `#ops/deploy` matches `tag: ops` — and the negative has to
    // agree, or `tag: ops` plus `not-tag: ops` returns something.
    let e = entry("TODO roll it out #ops/deploy", &[]);
    assert!(hits(&e, &Filter::Tag(tag("ops"))));
    assert!(!hits(&e, &not(Filter::Tag(tag("ops")))));
}

#[test]
fn tag_and_not_tag_on_the_same_name_can_never_both_match() {
    // The complement law, over the shapes most likely to break it.
    for text in [
        "plain text",
        "TODO ship #ops",
        "TODO ship #ops/deploy",
        "TODO ship #opsec",
        "TODO ship **#ops** bold",
        "TODO ship `#ops` in code",
        "TODO ship #Ops uppercase",
    ] {
        let e = entry(text, &[]);
        assert_ne!(
            hits(&e, &Filter::Tag(tag("ops"))),
            hits(&e, &not(Filter::Tag(tag("ops")))),
            "tag/not-tag disagree on {text:?}"
        );
    }
}

#[test]
fn the_fold_gate_stays_a_superset_of_the_tokenizer() {
    // `has_tag` short-circuits on `text_fold.contains(needle)`, and
    // that is only sound while a boundary match implies a substring
    // match on the folded text. `to_lowercase` is context-free per
    // character with one exception — `Σ` folds to `ς` at the end of
    // a word and `σ` otherwise, decided by what follows. The needle
    // is folded from the name alone, the haystack from the whole
    // block, so in principle they could disagree and the gate could
    // hide a block the tokenizer matches (a dropped `tag:` result,
    // a kept `not-tag:` one).
    //
    // They cannot, and this is why: every character that keeps a
    // preceding `Σ` non-final is also a character the tokenizer
    // accepts *into* the tag name, so the name never ends there and
    // both sides fold the same bytes. Brute-forced rather than
    // argued, because it rests on Unicode's Cased set lining up
    // with Rust's `is_alphanumeric` — true today, nobody's promise.
    let divergent: Vec<char> = (0u32..0x11_0000)
        .filter_map(char::from_u32)
        .filter(|c| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '/')))
        .filter(|c| format!("ΑΣ{c}").to_lowercase().starts_with("ασ"))
        .collect();
    assert!(
        divergent.is_empty(),
        "the gate can now hide a tag the tokenizer matches, after: {divergent:?}"
    );

    // The ordinary Greek path, so the test is not vacuous.
    let e = entry("TODO ship #ΑΣ", &[]);
    assert!(hits(&e, &Filter::Tag(tag("ΑΣ"))));
    assert!(!hits(&e, &not(Filter::Tag(tag("ΑΣ")))));
}

#[test]
fn a_hash_in_a_code_span_is_not_a_tag_to_either_side() {
    let e = entry("TODO run `#research` literally", &[]);
    assert!(!hits(&e, &Filter::Tag(tag("research"))));
    assert!(hits(&e, &not(Filter::Tag(tag("research")))));
}

#[test]
fn not_prop_on_a_bare_key_drops_any_block_carrying_it() {
    let e = entry("TODO ship it", &[("status", "parked")]);
    assert!(hits(&e, &Filter::Prop(prop("status"))));
    assert!(!hits(&e, &not(Filter::Prop(prop("status")))));
}

#[test]
fn not_prop_on_a_key_value_pair_only_drops_that_pair() {
    let e = entry("TODO ship it", &[("status", "parked")]);
    // A different value on the same key is not the pair excluded.
    assert!(hits(&e, &not(Filter::Prop(prop("status: done")))));
    assert!(!hits(&e, &not(Filter::Prop(prop("status: parked")))));
}

#[test]
fn prop_matching_folds_case_on_both_halves() {
    let e = entry("TODO ship it", &[("Status", "Done")]);
    assert!(hits(&e, &Filter::Prop(prop("status: done"))));
    assert!(hits(&e, &Filter::Prop(prop("STATUS: DONE"))));
}

#[test]
fn a_block_with_no_properties_survives_every_not_prop() {
    let e = entry("TODO ship it", &[]);
    assert!(hits(&e, &not(Filter::Prop(prop("status")))));
    assert!(hits(&e, &not(Filter::Prop(prop("status: done")))));
    assert!(!hits(&e, &Filter::Prop(prop("status"))));
}

#[test]
fn no_filter_and_its_negation_can_both_match() {
    // The complement law, over every directive rather than the two
    // that happened to ship first. `Filter::Not` makes it true by
    // construction; this is what fails if somebody reintroduces a
    // hand-written `NotFoo` arm with its own matcher.
    let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
    let cases = [
        Filter::Status(StatusFilter::Todo),
        Filter::Status(StatusFilter::Open),
        Filter::Tag(tag("ops")),
        Filter::Prop(prop("status: done")),
        Filter::Kind(KindFilter::Journal),
        Filter::Since(7),
        Filter::Text("ship".into()),
        Filter::Before("due".into(), NaiveDate::from_ymd_opt(2025, 7, 1).unwrap()),
        Filter::Page("notes".into()),
    ];
    let blocks = [
        entry("TODO ship #ops", &[("status", "done")]),
        entry("DONE ship #opsec", &[]),
        entry("plain text", &[("status", "parked")]),
    ];
    for f in &cases {
        for e in &blocks {
            let (status, _) = split_todo(&e.text);
            for journal in [Some(true), Some(false), None] {
                let yes = matches(f, e, status, journal, &today);
                let no = matches(&not(f.clone()), e, status, journal, &today);
                assert_ne!(yes, no, "{f:?} vs its negation on {:?}", e.text);
            }
        }
    }
}

#[test]
fn prop_and_not_prop_on_the_same_target_can_never_both_match() {
    for props in [
        vec![],
        vec![("status", "done")],
        vec![("status", "parked")],
        vec![("priority", "p1")],
        vec![("status", "done"), ("status", "parked")],
    ] {
        let e = entry("TODO ship it", &props);
        for target in ["status", "status: done"] {
            assert_ne!(
                hits(&e, &Filter::Prop(prop(target))),
                hits(&e, &not(Filter::Prop(prop(target)))),
                "prop/not-prop disagree on {props:?} for {target:?}"
            );
        }
    }
}

#[test]
fn page_filter_matches_slug() {
    // The `entry` helper always hosts on "notes".
    let e = entry("TODO thing", &[]);
    assert!(hits(&e, &Filter::Page("notes".into())));
    assert!(!hits(&e, &Filter::Page("archive".into())));
}

#[test]
fn before_matches_date_before_threshold() {
    let e = entry("TODO fix bug", &[("due", "2025-06-15")]);
    let threshold = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(hits(&e, &Filter::Before("due".into(), threshold)));
}

#[test]
fn before_excludes_date_after_threshold() {
    let e = entry("TODO fix bug", &[("due", "2025-08-01")]);
    let threshold = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(!hits(&e, &Filter::Before("due".into(), threshold)));
}

#[test]
fn after_matches_date_after_threshold() {
    let e = entry("TODO plan trip", &[("due", "2025-08-15")]);
    let threshold = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(hits(&e, &Filter::After("due".into(), threshold)));
}

#[test]
fn after_excludes_date_before_threshold() {
    let e = entry("TODO plan trip", &[("due", "2025-06-01")]);
    let threshold = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(!hits(&e, &Filter::After("due".into(), threshold)));
}

#[test]
fn before_excludes_non_date_values() {
    let e = entry("TODO task", &[("due", "high")]);
    let threshold = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(!hits(&e, &Filter::Before("due".into(), threshold)));
}

#[test]
fn before_excludes_missing_property() {
    let e = entry("TODO task", &[("priority", "high")]);
    let threshold = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(!hits(&e, &Filter::Before("due".into(), threshold)));
}

#[test]
fn before_is_case_insensitive_on_key() {
    // The index folds the block's key; the parser folds the filter
    // key — both halves meet lowercase.
    let e = entry("TODO task", &[("Due", "2025-06-01")]);
    let threshold = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(hits(&e, &Filter::Before("due".into(), threshold)));
}

#[test]
fn a_block_without_the_sort_date_sorts_last() {
    let mut hits = vec![
        hit("blk-no-due", vec![]),
        hit("blk-early", vec![("due".into(), "2025-07-01".into())]),
    ];
    sort_hits(&mut hits, &[SortKey::Prop("due".into())]);
    assert_eq!(hits[0].handle, "blk-early");
    assert_eq!(hits[1].handle, "blk-no-due");
}

#[test]
fn a_block_with_a_non_date_sort_value_sorts_last() {
    let mut hits = vec![
        hit("blk-non-date", vec![("due".into(), "high".into())]),
        hit("blk-dated", vec![("due".into(), "2025-07-01".into())]),
    ];
    sort_hits(&mut hits, &[SortKey::Prop("due".into())]);
    assert_eq!(hits[0].handle, "blk-dated");
    assert_eq!(hits[1].handle, "blk-non-date");
}

#[test]
fn a_block_with_two_dates_sorts_by_the_earliest() {
    // A second `due::` line must not silently move the block to
    // whichever line the file happens to list last.
    let mut hits = vec![
        hit(
            "blk-two",
            vec![
                ("due".into(), "2025-09-01".into()),
                ("due".into(), "2025-06-01".into()),
            ],
        ),
        hit("blk-one", vec![("due".into(), "2025-07-01".into())]),
    ];
    sort_hits(&mut hits, &[SortKey::Prop("due".into())]);
    assert_eq!(hits[0].handle, "blk-two");
}

/// A hit with only the sort-relevant fields filled.
fn hit(handle: &str, properties: Vec<(String, String)>) -> Hit {
    Hit {
        id: String::new(),
        handle: handle.into(),
        page_slug: "notes".into(),
        status: None,
        text: "x".into(),
        properties,
    }
}
