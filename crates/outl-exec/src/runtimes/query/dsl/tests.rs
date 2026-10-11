//! Unit tests for the query DSL parser.
//!
//! Split out of `dsl.rs` for the file-size ratchet, the same way
//! `outline/tests.rs` was: the seam is the `#[cfg(test)]` boundary,
//! never a line inside the parser itself.

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
fn every_filter_key_has_a_negative() {
    // The point of the generic `Not`: this list needs no
    // per-filter wiring, so a new directive cannot ship without
    // its negative.
    for (pos, neg) in [
        ("status: todo", "not-status: todo"),
        ("tag: ops", "not-tag: ops"),
        ("prop: status", "not-prop: status"),
        ("kind: journal", "not-kind: journal"),
        ("since: 7d", "not-since: 7d"),
        ("text: deploy", "not-text: deploy"),
    ] {
        let p = parse(pos).unwrap_or_else(|e| panic!("{pos}: {e}"));
        let n = parse(neg).unwrap_or_else(|e| panic!("{neg}: {e}"));
        assert_eq!(p.filters.len(), 1);
        assert_eq!(n.filters.len(), 1);
        assert!(
            matches!(&n.filters[0], Filter::Not(_)),
            "{neg} must parse to a Not"
        );
    }
}

#[test]
fn sort_and_limit_have_no_negative() {
    // They are not filters; `not-sort` is a typo, not a reversed
    // ordering, and reading it as one would be a silent surprise.
    assert!(parse("not-sort: page").is_err());
    assert!(parse("not-limit: 10").is_err());
}

#[test]
fn a_double_negative_is_an_unknown_key_not_a_positive() {
    let err = parse("not-not-tag: ops").unwrap_err();
    assert!(err.msg.contains("not-not-tag"), "got {}", err.msg);
}

#[test]
fn an_unknown_negative_key_is_reported_as_the_user_wrote_it() {
    let err = parse("not-bogus: x").unwrap_err();
    assert!(err.msg.contains("'not-bogus'"), "got {}", err.msg);
}

#[test]
fn an_empty_text_needle_is_rejected_on_both_sides() {
    // Empty is a substring of everything: `text:` would match the
    // whole workspace and `not-text:` would erase it.
    assert!(parse("text:").is_err());
    assert!(parse("not-text:").is_err());
}

#[test]
fn parses_not_tag() {
    let q = parse("not-tag: research").unwrap();
    let Filter::Not(inner) = &q.filters[0] else {
        panic!("expected Not, got {:?}", q.filters[0]);
    };
    let Filter::Tag(t) = inner.as_ref() else {
        panic!("expected Not(Tag), got {inner:?}");
    };
    assert_eq!(t.name(), "research");
    assert_eq!(t.needle(), "#research");
}

#[test]
fn repeated_not_tag_lines_each_become_a_filter() {
    // The documented way to exclude several tags: repeated lines,
    // ANDed like every other directive.
    let q = parse("not-tag: research\nnot-tag: future\nnot-tag: someday").unwrap();
    assert_eq!(q.filters.len(), 3);
    assert!(q.filters.iter().all(|f| matches!(f, Filter::Not(_))));
}

#[test]
fn a_leading_hash_on_a_tag_is_tolerated() {
    // `#research` is how the tag is spelled everywhere else; the
    // DSL should not punish copying it across.
    let q = parse("not-tag: #research").unwrap();
    let Filter::Not(inner) = &q.filters[0] else {
        panic!("expected Not");
    };
    let Filter::Tag(t) = inner.as_ref() else {
        panic!("expected Not(Tag)");
    };
    assert_eq!(t.name(), "research");
}

#[test]
fn an_empty_tag_is_rejected_rather_than_matching_everything() {
    // `not-tag:` with no name used to be expressible and would
    // have dropped every tagged block. Refuse instead.
    assert!(parse("tag:").is_err());
    assert!(parse("not-tag:").is_err());
    assert!(parse("not-tag: #").is_err());
}

#[test]
fn parses_prop_key_only_and_key_value() {
    let q = parse("prop: status").unwrap();
    let Filter::Prop(p) = &q.filters[0] else {
        panic!("expected Prop");
    };
    assert_eq!(p.key, "status");
    assert_eq!(p.value, None);

    let q = parse("not-prop: status: done").unwrap();
    let Filter::Not(inner) = &q.filters[0] else {
        panic!("expected Not");
    };
    let Filter::Prop(p) = inner.as_ref() else {
        panic!("expected Not(Prop)");
    };
    assert_eq!(p.key, "status");
    assert_eq!(p.value.as_deref(), Some("done"));
}

#[test]
fn prop_accepts_the_markdown_double_colon_spelling() {
    let q = parse("prop: status:: done").unwrap();
    let Filter::Prop(p) = &q.filters[0] else {
        panic!("expected Prop");
    };
    assert_eq!(p.key, "status");
    assert_eq!(p.value.as_deref(), Some("done"));
}

#[test]
fn prop_folds_case_like_the_index_does() {
    let q = parse("prop: Status: Done").unwrap();
    let Filter::Prop(p) = &q.filters[0] else {
        panic!("expected Prop");
    };
    assert_eq!(p.key, "status");
    assert_eq!(p.value.as_deref(), Some("done"));
}

#[test]
fn a_dangling_colon_on_prop_is_an_error_not_a_wildcard() {
    // `not-prop: status:` reading as "any status" would quietly
    // drop far more than the user asked for.
    assert!(parse("prop: status:").is_err());
    assert!(parse("not-prop: status:").is_err());
    assert!(parse("prop:").is_err());
}

#[test]
fn a_tag_name_outside_the_tokenizer_alphabet_is_rejected() {
    // No trailing comments in this DSL, so the whole tail becomes
    // the name. Accepting it would build a filter that can never
    // equal a tag token: "no results" for `tag:`, and — worse —
    // "excluded nothing" for `not-tag:`.
    assert!(parse("not-tag: research # parked stuff").is_err());
    assert!(parse("tag: \"research\"").is_err());
    assert!(parse("not-tag: two words").is_err());
    // The alphabet the tokenizer does accept still parses.
    assert!(parse("tag: ops/deploy-eu_2").is_ok());
}

#[test]
fn since_reports_a_multibyte_unit_instead_of_panicking() {
    // `v.len() - 1` is not a char boundary here, and the runtime
    // auto-runs on page load — a panic would take the host down
    // every time the page is opened.
    let err = parse("since: 3м").unwrap_err();
    assert!(err.msg.contains("unknown unit"), "got {}", err.msg);
    assert!(parse("since: 7日").is_err());
    assert!(parse("since: 2w").is_ok());
}

#[test]
fn parse_errors_carry_the_line_number() {
    let err = parse("status: todo\nnot-tag:").unwrap_err();
    assert_eq!(err.line, 2);
}

#[test]
fn parses_page_filter() {
    let q = parse("page: inbox").unwrap();
    assert_eq!(q.filters.len(), 1);
    assert!(matches!(&q.filters[0], Filter::Page(s) if s == "inbox"));
    assert!(parse("page:").is_err());
}

#[test]
fn parses_before_iso_date() {
    let q = parse("before: due 2025-07-01").unwrap();
    assert_eq!(q.filters.len(), 1);
    let expected = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    assert!(matches!(&q.filters[0], Filter::Before(k, d) if k == "due" && *d == expected));
}

#[test]
fn parses_after_iso_date() {
    let q = parse("after: deadline 2025-08-15").unwrap();
    assert_eq!(q.filters.len(), 1);
    let expected = NaiveDate::from_ymd_opt(2025, 8, 15).unwrap();
    assert!(matches!(&q.filters[0], Filter::After(k, d) if k == "deadline" && *d == expected));
}

#[test]
fn parses_after_today() {
    let q = parse("after: scheduled tomorrow").unwrap();
    assert_eq!(q.filters.len(), 1);
    assert!(matches!(&q.filters[0], Filter::After(k, _) if k == "scheduled"));
}

#[test]
fn before_requires_date() {
    assert!(parse("before: due notadate").is_err());
}

#[test]
fn before_requires_key_and_date() {
    assert!(parse("before: ").is_err());
}

#[test]
fn before_still_accepts_the_inner_colon_form() {
    // `before: due: +7d` (colon between key and date) still parses —
    // docs lead with the space form, the colon alias stays tolerated.
    let q = parse("before: due: +7d").unwrap();
    assert_eq!(q.filters.len(), 1);
    assert!(matches!(&q.filters[0], Filter::Before(k, _) if k == "due"));
}

#[test]
fn parses_sort_by_property() {
    let q = parse("sort: due").unwrap();
    assert_eq!(q.sort.len(), 1);
    assert_eq!(
        q.sort[0],
        Sort {
            key: SortKey::Prop("due".into()),
            dir: Dir::Asc,
        }
    );
}

#[test]
fn parses_sort_mixed_keywords_and_property() {
    let q = parse("sort: page, due").unwrap();
    assert_eq!(q.sort.len(), 2);
    assert_eq!(q.sort[0].key, SortKey::Page);
    assert_eq!(q.sort[1].key, SortKey::Prop("due".into()));
}

#[test]
fn parses_sort_direction_suffix() {
    let q = parse("sort: due:desc, created:asc").unwrap();
    assert_eq!(
        q.sort,
        [
            Sort {
                key: SortKey::Prop("due".into()),
                dir: Dir::Desc,
            },
            Sort {
                key: SortKey::Created,
                dir: Dir::Asc,
            },
        ]
    );
    assert_eq!(
        parse("sort: journal-date").unwrap().sort[0].key,
        SortKey::JournalDate
    );
}

#[test]
fn a_bad_sort_direction_is_a_line_error() {
    // Strict like the rest of the vocabulary: `DESC` is a typo, not
    // a case-tolerant `desc`.
    let err = parse("status: todo\nsort: due:DESC").unwrap_err();
    assert_eq!(err.line, 2);
    assert!(err.msg.contains("'asc' or 'desc'"), "got {}", err.msg);
    // A colon with nothing after it names a missing direction, not
    // a property literally called `due:`.
    assert!(parse("sort: due:").is_err());
    // A direction is never a hyphen prefix: `-due` is a property key.
    assert_eq!(
        parse("sort: -due").unwrap().sort[0].key,
        SortKey::Prop("-due".into())
    );
}

#[test]
fn huge_relative_offset_errors_instead_of_panicking() {
    // Two distinct overflow paths, both must fall through to a parse
    // error rather than panic the auto-run render path:
    //   +999999999999d  overflows the time span (Duration::try_days)
    //   +100000000d     overflows the NaiveDate range (~year 275000)
    assert!(parse("before: due +999999999999d").is_err());
    assert!(parse("before: due +100000000d").is_err());
    assert!(parse("before: due +999999999999w").is_err());
    // A merely-large offset still parses (chrono NaiveDate reaches
    // year 262143, so ~year 10220 is representable).
    assert!(parse("before: due +3000000d").is_ok());
}

#[test]
fn huge_month_offset_errors_not_truncates() {
    // `u32::try_from` guards the Months::new cast: a count beyond
    // u32::MAX must error, not silently wrap to a small month count.
    assert!(parse("before: due +4294967296m").is_err());
}

#[test]
fn moderate_relative_offset_still_parses() {
    let q = parse("before: due +3650d").unwrap();
    assert!(matches!(&q.filters[0], Filter::Before(k, _) if k == "due"));
}

#[test]
fn parses_offset_directive() {
    let q = parse("sort: due\nlimit: 5\noffset: 10").unwrap();
    assert_eq!(q.limit, Some(5));
    assert_eq!(q.offset, Some(10));
    // Absent stays `None` — no offset means start at the top.
    assert_eq!(parse("tag: ops").unwrap().offset, None);
    assert!(parse("offset: -1").is_err());
    assert!(parse("offset: soon").is_err());
}
