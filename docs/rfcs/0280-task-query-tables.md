# RFC 0280 — Task query tables: scanning tasks across projects

| | |
|---|---|
| **Status** | Proposed |
| **Issue** | [#139](https://github.com/outlmd/outl/issues/139) (parent) — table issue to be filed |
| **PR** | none yet |
| **Date** | 2026-09-27 |
| **Reference doc** | [query.md](../query.md) |
| **Invariant** | root `CLAUDE.md` invariants 1, 2, 8, 10, 12 |
| **Guarded by** | proposed — see [Guarded by](#guarded-by) |

## Why

` ```query ` renders every result as a **flat bullet list of live embed references**
(`OutputFormat::Embeds`, `crates/outl-exec/src/result_block.rs:109`).
That shape answers "show me the matching blocks".
It does not answer "give me a task board I can scan across a dozen projects",
because there is no way to see a task's page, status, tags and due date
*side by side* across dozens of blocks, and no way to roll them up per project.

SilverBullet's task-query table view does exactly this.
outl already has the data — it is simply discarded at render time.
`engine::Hit` (`crates/outl-exec/src/runtimes/query.rs:933`) carries `status`, `text`, `page_slug` and `properties: Vec<(String, String)>`,
and tags are already scanned for filtering.
The table feature surfaces what the engine already computed; it does not require a new index or a new data model.

## What we chose

A ` ```query ` fence can render its results as a **multi-column, grouped table**,
scannable across every page and journal at once, while staying a **live view**
exactly like today's embeds.

Four stacked changes, each independently useful:

1. **Table render mode** — `render: table` + a `columns:` spec, plus a new `OutputFormat::Table`.
2. **Boolean OR and the long-planned `group:`** — one table can show `#ops` *and* `#design`, rolled up per page.
3. **Incremental index** — the scale prerequisite; cross-project scanning is precisely the workload the current full rebuild hurts.
4. **Computed columns** — `overdue`, `days-left`, `tags`, a status chip, and a per-group rollup.

Single owner stays `crates/outl-exec/src/runtimes/query.rs` for parsing and the engine;
the new rendering lives where each client already renders embeds.

### Rows are live references, never text copies

This is the load-bearing decision, and it is not actually a choice — it is inherited.
Every query result today is already live: an embed row is a `!((blk-…))` reference, not a copy,
and that is precisely why toggling a TODO on the original propagates everywhere the query surfaces.

A table preserves the same property by storing, per row, only the block handle (the embed child already stored today),
and **re-deriving every cell from `WorkspaceIndex` at paint time**.
No markdown-cell table is ever written into a `.md`.
The materialized result is therefore the existing `Embeds` write shape — child bullets of handles under a result header —
with the header additionally carrying `table` and the `columns:` spec, which the client re-reads to render the grid.

This keeps all three properties that make embeds correct:

- **live view** — edit or toggle the source, the table reflects it on the next auto-run;
- **`.md` stays clean** (invariants 1 and 2) — no task text duplicated into a markdown table, nothing schema-shaped in the sidecar;
- **op log is source of truth** — toggling a checkbox in a cell routes through the existing
  `cycle_embed_source_status` path (`crates/outl-tui/src/actions/block/metadata.rs:147`), the same op `Ctrl+T` already emits on an embed row.

### Grammar (final)

A column name resolves in this fixed priority order:

```
built-in   → status | text | page | kind          (the enum/fixed columns every hit carries)
computed   → overdue | days-left                   (derived from a date property; change 4)
property   → any other name → a block property key  (the same keys `prop` / `before` / `after` use)
```

A **built-in or computed name always wins** over a block property of the same spelling.
To force a property that collides with a built-in (`text`, `page`, `status`, `kind`) or a computed name (`tags`, `overdue`, `days-left`),
the `prop:` prefix selects the property explicitly:

```
columns: status, text, prop:tags, overdue
```

Rules settled during review:

| Question | Decision |
|----------|----------|
| Collision with a built-in/computed name | Built-in/computed wins; force the property with `prop:` |
| `tags` column shape | **Comma list** in one cell (`#ops #urgent`), not one column per tag |
| Requested column with no data on any hit | **Render the empty column** — a valid property that no matched block carries yields a blank cell, not an error |
| Bad built-in name (`cloumns:`, `stat`) | **Parse error** — it can never resolve, same class as today's `rejects_unknown_key` |
| `render: table` with no `columns:` | **Valid** — defaults to `status, text, page`, the three columns every hit already carries |
| `columns:` with no `render: table` | **Parse error** ("`columns:` requires `render: table`") — a bare column spec has no embeds grid to attach to |

### The two visual states

`render: table` with an explicit spec is a dashboard; with no spec it is a tidier version of today's flat list.
That difference is why the zero-config default is valid rather than an error.

`columns: status, text, overdue, tags` + `group: page` (a dashboard):
```
▾ ops
  STATUS  TEXT                       OVERDUE      TAGS
  todo    rotate vaultwarden certs   today        #ops #urgent
  doing   split ingest worker        +3d          #ops
  todo    audit firewall rules       —            #ops #infra
▾ design
  STATUS  TEXT                       OVERDUE      TAGS
  todo    rethink sidebar IA         +12d         #design
  todo    audit color tokens         -2d          #design #a11y
```

`render: table` alone (defaults to `status, text, page`):
```
STATUS  TEXT                       PAGE
todo    rotate vaultwarden certs   ops
doing   split ingest worker        ops
todo    audit firewall rules       ops
todo    rethink sidebar IA         design
todo    audit color tokens         design
```

### Boolean OR

Today every directive ANDs, so a table cannot show two tag sets at once.
Value filters accept `|` as disjunction — `tag: ops | design`, and by symmetry `status: todo | doing`, `prop: priority high | medium`.
A disjunction over *filters* (not values) stays out of scope; the value-level OR is what the table use case needs.

### `group:`

`group: page` emits a header row and a count per page (the `▾ ops` / `▾ design` rows above).
Grouping is a **projection** concern, so it is computed client-side over the already-ordered row list — it never touches the op log or the write format.
This replaces the "planned, not implemented" `group:` row in [`docs/query.md`](../query.md).

## The opposite direction

Invariants say a `.md` ↔ tree fix must state what happens in the mirror direction before merge.
The mirror of a table is **a checkbox toggled while the table is open**, and two failures are visible from here.

- **`.md` ahead of the table** — a row shows a task whose source text was edited on another device between the index snapshot and the paint.
  Because cells are re-derived from the index (never stored), the stale text self-heals on the next auto-run; the table cannot *hide* content the way a hash-gated re-projection could (invariant 8). The row carries a handle, not content, so there is nothing stale to overwrite.
- **table ahead of `.md`** — a checkbox click emits the same `cycle_embed_source_status` op the source block's own `Ctrl+T` emits, through `Workspace::apply`. It cannot write a cell, so it cannot create `.md` content outside the op log. A click on a source block that a concurrent op just deleted must no-op cleanly (the id is gone at apply time), not resurrect it — this is a required test, not a hope.

So the table inherits the invariant-8 safety that embeds already have, **because it reuses the embed write path verbatim.**
A design that materialized markdown cells instead would reintroduce the whole invariant-8 loss surface for zero gain — which is the reason it is rejected, not a style preference.

## Invariant 12 — the capability has to have a verdict

Rendering a query result as a grid is a **capability**, not an `Action` — query fences auto-run and have no run gesture — so it belongs in `outl_shortcuts::capability_support`, alongside the existing ten `Capability` variants.

A new `Capability::QueryTable` row, exhaustive over the three clients:

| Client | Support | Reason carried in the catalog |
|--------|---------|-------------------------------|
| TUI | `Full` | grid renderer beside `emit_embedded_children` |
| Desktop | `Full` | `<QueryTable>` in `@outl/shared`, consumed where `<EmbeddedSubtree>` is today |
| Mobile | `Missing` | "Query results render as text on mobile; table view is not wired yet." |

`docs/client-parity.md` is regenerated from that match and pinned by `the_parity_doc_matches_the_code`, so the verdict cannot rot.

### A pre-existing gap this feature would inherit, and records

Query embeds are wired on **desktop only**.
`resolve_embeds` is registered on mobile via `exec_commands!`, but no mobile component calls `resolveEmbeds` / renders `EmbeddedSubtree` —
mobile auto-runs a query, updates the tree, and never resolves the handles back into rows.
That gap is currently **invisible to the parity matrix**: query embeds have neither an `Action` (auto-run, no gesture) nor a `Capability`,
so invariant 12 has nothing to check.

Adding `Capability::QueryTable` and having `all_matches_the_enum` bump `EXPECTED_10` forces the verdict to be *recorded* even where it is not *delivered* —
which is exactly the invariant-12 point: a capability with no recorded verdict is a gap nobody owns.
The mobile auto-run-embeds gap is real and mobile is upstream-owned today, so this RFC **scopes to TUI + desktop and records the mobile gap**; closing it is tracked separately, not here.

## The incremental index — and who was standing on the full rebuild

Invariants 9 and 10 apply to change 3.
Today `WorkspaceIndex` is full-walk only: `index.rs` rebuilds by walking `pages/` + `journals/`, and the `query` runtime falls back to a disk rebuild per fence when no index is injected (`crates/outl-exec/CLAUDE.md`, "The `query` runtime").
Cross-project scanning — every task, every page, one table — is the worst case for a per-load full rebuild.

The change: update the affected page's block entries on `apply_op` / page save instead of re-walking, and drop the disk rebuild for callers that already hold a `&Workspace`.

Before it lands, invariant 10's checklist, because "incremental index" moves authority away from "always rebuild fresh":

1. **Who consumed the always-fresh index?** Backlinks and the TUI auto-run loop both read `ctx.index`.
   An always-rebuilt index could never be stale; an incremental one can, so every consumer must be named and its staleness tolerance checked.
2. **What does each loser do now?** A consumer that can no longer assume freshness needs an invalidation signal on `apply_op`.
3. **Does the guard die with the thing it authorises?** If a page's `.md` is edited out-of-band (iCloud, a peer, an external editor), an incremental index keyed only on `apply_op` goes stale silently — the file-watcher/reconcile path must also drop-and-rebuild that page's entries, or the incremental index is a wall.
4. **Which exemptions existed because the rebuild always ran?** The `ctx.index == None` disk fallback is one; it exists to serve embedders holding no `Workspace`. Keeping it is fine, but it must stay a genuine fallback, not the hot path a `None` slips into by omission.
5. **Did the name already mean something?** `derive` currently means "full walk". Incremental update is a different verb and needs its own entry point, not a redefinition of `derive`.

A full rebuild remains a supported operation (and the correctness fallback); incremental is the hot path layered on top.

## Why not the alternatives

**A materialized markdown table written into the `.md`.**
Cells would be text copies.
The table would go stale against the op log and could *hide* content (the invariant-8 loss direction), and would fail "clean `.md`" the moment a cell held a date chip a re-parse could not round-trip.
This is the option that buys the least and costs the most; rejected for the reason above, not taste.

**A SQLite / rusqlite task index.**
Reintroduces a binary log format, which cross-device sync depends on being able to *not* have (per-actor append-only JSONL is the contract). The task table is a projection over `WorkspaceIndex`, not a second source of truth.

**Full datalog / SLIQ (SilverBullet's language).**
Expressive enough to also change the whole query *format* into a permanent public contract, for a task-scanning need the existing line-DSL plus `columns:`/`group:`/`|` already covers.
Stays consistent with [RFC 0139](0139-query-language.md): line-oriented, one-directive-per-line, backwards compatible.

**One column per tag (a boolean tag grid).**
Scannable but unbounded in width and unstable in ordering as tags churn. The comma-list `tags` column is v1; a dynamic grid, if ever wanted, is additive later.

## Scope

**In:** `OutputFormat::Table`, `render:`/`columns:`/`group:`/value-`|`, computed columns, TUI grid renderer, `<QueryTable>` in `@outl/shared` + desktop wiring, incremental index, `Capability::QueryTable`.

**Out (recorded, not closed):** mobile rendering — upstream-owned today; recorded as `Capability::QueryTable → Missing` on mobile and as its own issue.
Inline `{{query: …}}` stays dead (unchanged from [RFC 0139](0139-query-language.md)).
Per-page op-log shards stay future work until the single-jsonl layout hits the 10k wall ([`docs/sync.md`](../sync.md)).

## Sequencing

1. **Incremental index** first — independent, de-risks the scale question every later change leans on.
2. **Table render mode** (`OutputFormat::Table`, DSL directives, TUI + desktop renderers, `Capability::QueryTable`).
3. **`group:` + value-OR** and **computed columns** build on the column model of change 1.

## Guarded by

Proposed tests to land with the implementation (RFC is `Proposed` until they exist):

- `parses_render_table`, `parses_columns_spec`, `defaults_columns_when_render_table_has_none`, `rejects_columns_without_render_table`, `rejects_unknown_column_name` (the `dsl` tests in `crates/outl-exec/src/runtimes/query.rs`).
- `column_resolution_prefers_builtin_over_property`, `prop_prefix_selects_the_property`, `missing_property_renders_empty_column` (engine `Hit` → cell mapping).
- `value_or_matches_either_tag`, `group_by_page_emits_a_header_per_slug`.
- `overdue_column_is_red_for_past_due`, `tags_column_is_a_comma_list` (computed columns).
- `toggling_a_table_row_emits_the_same_op_as_ctrl_t_on_the_source` and `toggling_a_deleted_source_block_is_a_clean_noop` (invariant 1, the opposite-direction failure).
- `a_table_row_holds_a_handle_not_text_so_it_cannot_hide_logged_content` (invariant 8 — the regression net this RFC exists to add).
- `incremental_index_reflects_an_apply_op_without_a_rebuild`, `an_out_of_band_md_edit_forces_that_page_to_rebuild` (invariant 10 — who was standing on the rebuild).
- `all_matches_the_enum` bumped for `Capability::QueryTable`, `the_parity_doc_matches_the_code`, and the TS `shortcuts.support.test.ts` desktop arm (invariant 12).
