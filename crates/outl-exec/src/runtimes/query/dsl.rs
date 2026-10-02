//! Query DSL parser — one `key: value` directive per line.
//!
//! Split out of the runtime module so the parser, the engine and the
//! public API each stay under the file-size ratchet. The grammar is
//! deliberately flat: every directive is a filter, filters are
//! implicitly ANDed, and there is no grouping or precedence to get
//! wrong (see [RFC 0139](../../../../../docs/rfcs/0139-query-language.md)).
//!
//! ## Negative filters
//!
//! **Every filter has a negative, and none of them has its own
//! matcher.** `not-<key>` parses `<key>` and wraps the result in
//! [`Filter::Not`], which the engine answers with one `!`. So a query
//! carrying both `tag: x` and `not-tag: x` returns nothing — not
//! because two implementations were kept in step, but because there
//! is only one.
//!
//! The same shape is why a **new** filter needs no negative-filter
//! work: add the variant and its match arm, and `not-<key>` is live.
//! A hand-written `NotFoo` variant is the thing to refuse in review —
//! it is a second opinion about what `foo` means, and the direction
//! it drifts is the one that silently removes results.

use std::fmt;

use chrono::NaiveDate;

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
    Tag(TagFilter),
    Prop(PropFilter),
    /// Date-property comparison: the block carries `key` as an ISO
    /// date strictly before / after the threshold.
    Before(String, NaiveDate),
    After(String, NaiveDate),
    /// Restrict hits to one hosting page, by slug.
    Page(String),
    Kind(KindFilter),
    Since(u32),
    Text(String),
    /// Negation of any of the above — what every `not-<key>` parses
    /// to. Boxed because the enum would otherwise be recursive by
    /// value; one allocation per negated directive, at parse time.
    ///
    /// The parser strips exactly one `not-` prefix, so `not-not-tag`
    /// is an unknown key rather than a double negative nobody meant.
    Not(Box<Filter>),
}

/// A `tag:` / `not-tag:` target: `#<name>`, lowercased.
///
/// Stored with the `#` already attached because that is the substring
/// the engine tests against each block's cached `text_fold` before
/// paying for the tokenizer. Building it per block would put an
/// allocation in the hot loop of a filter that auto-runs on page load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagFilter(String);

impl TagFilter {
    pub fn new(name: &str) -> Result<Self, String> {
        let name = name.trim().trim_start_matches('#').trim();
        if name.is_empty() {
            return Err("tag requires a name, e.g. 'tag: ops'".into());
        }
        // The tokenizer's tag alphabet (`outl_md`'s `reference.rs`).
        // A name outside it can never equal a tag token, so the filter
        // would parse and then quietly match nothing — which reads as
        // "no results" for `tag:` and as "excluded nothing" for
        // `not-tag:`. The line `not-tag: research # parked` is the
        // way in: the DSL has no trailing comments, so the whole tail
        // becomes the name.
        if let Some(bad) = name
            .chars()
            .find(|c| !(c.is_alphanumeric() || matches!(c, '-' | '_' | '/')))
        {
            return Err(format!(
                "tag '{name}' contains {bad:?}, which cannot appear in a tag \
                 (letters, digits, '-', '_' and '/' only)"
            ));
        }
        Ok(Self(format!("#{}", name.to_lowercase())))
    }

    /// `#<name>` — the cheap substring gate.
    pub fn needle(&self) -> &str {
        &self.0
    }

    /// The tag name, lowercased, without the `#`.
    pub fn name(&self) -> &str {
        &self.0[1..]
    }
}

/// A `prop:` / `not-prop:` target: a property key, optionally narrowed
/// to one value.
///
/// Both halves are lowercased at parse time to match
/// `BlockEntry::properties`, which the index folds once on the way in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropFilter {
    /// Property key, lowercased.
    pub key: String,
    /// Property value, lowercased. `None` means "any value" — the
    /// filter then asks only whether the key is present.
    pub value: Option<String>,
}

impl PropFilter {
    /// Parse `key` or `key: value`.
    ///
    /// `key:: value` is accepted too: that is how a property is spelled
    /// in the `.md`, so a user copying one across into a query should
    /// not have to remember to drop a colon.
    pub fn new(raw: &str) -> Result<Self, String> {
        let raw = raw.trim();
        let (key, value) = match raw.split_once(':') {
            Some((k, v)) => (k.trim(), Some(v.trim_start_matches(':').trim())),
            None => (raw, None),
        };
        if key.is_empty() {
            return Err("prop requires a key, e.g. 'prop: status' or 'prop: status: done'".into());
        }
        // An empty value is a typo, not "any value": silently widening
        // `not-prop: status:` to "drop everything carrying a status"
        // is exactly the over-exclusion a negative filter must not do.
        if let Some(v) = value {
            if v.is_empty() {
                return Err(format!(
                    "prop '{key}:' has no value — drop the colon to match any value"
                ));
            }
        }
        Ok(Self {
            key: key.to_lowercase(),
            value: value.map(str::to_lowercase),
        })
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortKey {
    Page,
    Status,
    Text,
    /// Sort by a block property parsed as an ISO date — `sort: due`.
    /// Anything that is not a known fixed key is read as a property
    /// name, so new properties need no parser change.
    Prop(String),
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

        let (key, value) = split_kv(line, i)?;
        let key = key.trim();
        let value = value.trim();

        // `sort` and `limit` are not filters, so there is nothing to
        // negate: `not-sort` falls through to the unknown-key error
        // below rather than being read as some reversed ordering.
        match key {
            "sort" => {
                for part in value.split(',') {
                    sort.push(parse_sort_key(part.trim(), i)?);
                }
                continue;
            }
            "limit" => {
                limit = Some(parse_usize(value, i)?);
                continue;
            }
            _ => {}
        }

        let (base, negated) = key
            .strip_prefix("not-")
            .map_or((key, false), |rest| (rest, true));
        let at = |msg: String| ParseError { line: i + 1, msg };
        let filter = match base {
            "status" => Filter::Status(parse_status(value, i)?),
            "tag" => Filter::Tag(TagFilter::new(value).map_err(at)?),
            "prop" => Filter::Prop(PropFilter::new(value).map_err(at)?),
            "before" => {
                let (k, d) = parse_prop_date(value, i)?;
                Filter::Before(k, d)
            }
            "after" => {
                let (k, d) = parse_prop_date(value, i)?;
                Filter::After(k, d)
            }
            "page" => Filter::Page(parse_page_slug(value, i)?),
            "kind" => Filter::Kind(parse_kind(value, i)?),
            "since" => Filter::Since(parse_duration(value, i)?),
            "text" => Filter::Text(parse_text(value, i)?),
            _ => {
                // Report the key the user wrote, `not-` and all.
                return Err(ParseError {
                    line: i + 1,
                    msg: format!("unknown key: '{key}'"),
                });
            }
        };
        filters.push(if negated {
            Filter::Not(Box::new(filter))
        } else {
            filter
        });
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
    parse_duration_str(v).map_err(|msg| ParseError {
        line: line_idx + 1,
        msg,
    })
}

/// Parse `Nd` / `Nw` / `Nm` into a day count, without line context.
///
/// Shared with the structured API, which has no lines to report.
pub fn parse_duration_str(v: &str) -> Result<u32, String> {
    // Split on the last *character*, not the last byte. `v.len() - 1`
    // is not a char boundary when the unit is multi-byte, and
    // `split_at` panics there — inside a runtime with
    // `auto_run() == true`, so `since: 3м` in one fence would take
    // down the TUI event loop on every load of that page.
    let Some(unit) = v.chars().next_back() else {
        return Err("since requires a duration like '7d', '2w', '3m'".into());
    };
    let num_str = &v[..v.len() - unit.len_utf8()];
    let n: u32 = num_str
        .parse()
        .map_err(|_| format!("since: invalid number in '{v}'"))?;
    match unit {
        'd' => Ok(n),
        'w' => Ok(n * 7),
        'm' => Ok(n * 30),
        _ => Err(format!("since: unknown unit '{unit}' (use d, w, or m)")),
    }
}

/// Parse a `text:` needle, refusing an empty one.
///
/// An empty needle is a substring of everything, so `text:` would
/// match every block and `not-text:` would drop every block. Both are
/// a typo answered with a whole workspace, or none of it.
fn parse_text(v: &str, line_idx: usize) -> Result<String, ParseError> {
    if v.is_empty() {
        return Err(ParseError {
            line: line_idx + 1,
            msg: "text requires something to search for, e.g. 'text: deploy'".into(),
        });
    }
    Ok(v.to_string())
}

fn parse_sort_key(v: &str, _line_idx: usize) -> Result<SortKey, ParseError> {
    match v {
        "page" => Ok(SortKey::Page),
        "status" => Ok(SortKey::Status),
        "text" => Ok(SortKey::Text),
        // Anything else is read as a property name — `sort: due`
        // orders by the block's `due::` property as an ISO date.
        // An empty key is still a typo worth reporting.
        "" => Err(ParseError {
            line: _line_idx + 1,
            msg: "sort: empty key".into(),
        }),
        // Folded: the block index lowercases property keys, so the
        // sort key has to fold too or `sort: Due` would match nothing.
        other => Ok(SortKey::Prop(other.to_lowercase())),
    }
}

/// Parse `key date` from a `before:` / `after:` directive value.
/// The date accepts ISO (`2025-07-01`) and relative forms — see
/// [`parse_date_threshold_value`].
fn parse_prop_date(v: &str, line_idx: usize) -> Result<(String, NaiveDate), ParseError> {
    let (key, date_str) = if let Some((k, val)) = v.split_once(':') {
        (k.trim(), val.trim())
    } else if let Some((k, val)) = v.split_once(' ') {
        (k.trim(), val.trim())
    } else {
        return Err(ParseError {
            line: line_idx + 1,
            msg: format!("before/after requires 'key date', got '{v}'"),
        });
    };
    if key.is_empty() || date_str.is_empty() {
        return Err(ParseError {
            line: line_idx + 1,
            msg: format!("before/after requires non-empty key and date, got '{v}'"),
        });
    }
    let today = chrono::Local::now().date_naive();
    let date = parse_date_threshold_value(date_str, today).ok_or_else(|| ParseError {
        line: line_idx + 1,
        msg: format!(
            "invalid date '{date_str}' (use ISO 2025-07-01, +7d, -2w, +1m, today, tomorrow, yesterday)"
        ),
    })?;
    // The block index lowercases property keys, so the threshold key
    // has to fold too — same rule `PropFilter` follows.
    Ok((key.to_lowercase(), date))
}

/// Parse a `page:` slug, refusing an empty one.
///
/// An empty slug would match no page at all and silently return
/// nothing, so a `page:` typo is an error rather than an empty list.
fn parse_page_slug(v: &str, line_idx: usize) -> Result<String, ParseError> {
    if v.is_empty() {
        return Err(ParseError {
            line: line_idx + 1,
            msg: "page requires a slug, e.g. 'page: inbox'".into(),
        });
    }
    Ok(v.to_string())
}

/// Parse a date threshold value: ISO, relative offsets, or keywords.
pub(crate) fn parse_date_threshold_value(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let s = s.trim();
    match s {
        "today" => return Some(today),
        "tomorrow" => return today.checked_add_signed(chrono::Duration::days(1)),
        "yesterday" => return today.checked_sub_signed(chrono::Duration::days(1)),
        _ => {}
    }
    if let Ok(d) = NaiveDate::parse_from_str(s, ISO_DATE_FMT) {
        return Some(d);
    }
    parse_relative_offset(s, today)
}

/// Parse a relative offset like `+7d`, `-2w`, `+1m` against `today`.
fn parse_relative_offset(s: &str, today: NaiveDate) -> Option<NaiveDate> {
    let (sign, rest) = if let Some(rest) = s.strip_prefix('+') {
        (1i64, rest)
    } else if let Some(rest) = s.strip_prefix('-') {
        (-1i64, rest)
    } else {
        return None;
    };
    let last = rest.chars().last()?;
    if !"dwm".contains(last) {
        return None;
    }
    let num_str = &rest[..rest.len() - last.len_utf8()];
    let n: i64 = num_str.parse().ok()?;
    let signed = sign.checked_mul(n)?;
    // Every branch is checked: a huge offset (`+999999999999d`) overflows
    // the time span and a merely-large one (`+3000000d`) overflows the
    // supported date range. Both are user-typed, so they must fall through
    // to a parse error rather than panic the auto-run render path.
    match last {
        'd' => chrono::Duration::try_days(signed).and_then(|d| today.checked_add_signed(d)),
        'w' => chrono::Duration::try_weeks(signed).and_then(|d| today.checked_add_signed(d)),
        'm' => {
            let months = chrono::Months::new(u32::try_from(signed.unsigned_abs()).ok()?);
            if signed >= 0 {
                today.checked_add_months(months)
            } else {
                today.checked_sub_months(months)
            }
        }
        _ => None,
    }
}

/// ISO date format shared by the parser and the engine.
pub(crate) const ISO_DATE_FMT: &str = "%Y-%m-%d";

fn parse_usize(v: &str, line_idx: usize) -> Result<usize, ParseError> {
    v.parse::<usize>().map_err(|_| ParseError {
        line: line_idx + 1,
        msg: format!("expected a number, got '{v}'"),
    })
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
        assert!(
            matches!(&q.filters[0], Filter::After(k, d) if k == "deadline" && *d == expected)
        );
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
        assert!(matches!(&q.sort[0], SortKey::Prop(k) if k == "due"));
    }

    #[test]
    fn parses_sort_mixed_keywords_and_property() {
        let q = parse("sort: page, due").unwrap();
        assert_eq!(q.sort.len(), 2);
        assert!(matches!(q.sort[0], SortKey::Page));
        assert!(matches!(&q.sort[1], SortKey::Prop(k) if k == "due"));
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
}
