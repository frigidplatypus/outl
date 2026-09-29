# RFC 0329 — A pipe table is one block, drawn as a grid where the client can

| | |
|---|---|
| **Status** | Accepted (TUI shipped; GUI renderers open) |
| **Issue** | [#329](https://github.com/outlmd/outl/issues/329) |
| **PR** | none yet |
| **Date** | 2026-09-27 |
| **Reference doc** | [markdown-format.md → Pipe tables](../markdown-format.md#pipe-tables); `crates/outl-md/CLAUDE.md` → "Pipe tables"; `crates/outl-tui/CLAUDE.md` → the table-render bullet |
| **Invariant** | root `CLAUDE.md` invariants 1, 2, 8, 12, 13 |
| **Guarded by** | see [Guarded by](#how-it-cannot-regress) |

## Why

A `.md` copied out of GitHub, a README, or another outliner carries pipe tables.
Imported into outl, a table fell through the outline grammar: it has no `- ` marker and no `key:: value`, so **every one of its lines** hit the depth-0 recovery arm and became a verbatim block of its own, each raising an `UnrecognizedBlockMarker` warning.
A nine-row table was nine orphan blocks and nine warnings; the TUI drew a wall of `!` glyphs where a grid should sit, and the parse banner lit up for content the user wrote correctly.

The table itself is not exotic — it is the single most common structure a user pastes in from anywhere on the web — and outl was the worst place on earth to look at one.

## What we chose

Two owners, split along the storage/render boundary, so nothing about the op log changes.

**Parse (`outl_md::table`) — recognise and collapse, never interpret.**
`looks_like_table_start` gates on a conservative predicate: a header row, a `|:--|--:|` delimiter row, and at least one data row, **all at column 0**.
`consume_table_block` then folds the whole run into **one** `OutlineNode` whose `text` is the rows, trimmed, joined by `\n` — the shape a fenced code block already uses.
The collapse happens in the depth-0 arm of `parse`; on `None` the helper leaves the cursor untouched and the existing verbatim recovery runs unchanged, so this PR cannot regress a non-table line.

**Render (`outl_tui::view::table`) — recover the grid, then draw it.**
`outl_md::parse_table_block(text)` returns `Option<Table>` (fields `alignments`, `header`, `rows`) only when the block's text is a *pure* table; anything else is `None` and renders as ordinary inline content.
`view::table::emit_table_lines` builds the aligned grid, reached from the `RenderMode::Pretty` branch of `emit_block_lines`.

**Why a whole table is one node, and why that is a fixpoint.**
It is the **continuation path** doing the round-trip, not a new one.
The first parse (a raw import) reads each row at column 0 and stores it trimmed; the renderer writes the first row after `- ` and the rest one level deeper; every later parse reassembles those indented continuation lines into the same trimmed, `\n`-joined text.
So `render → parse` is stable, and — the invariant-8 point — **storage stays byte-verbatim**: alignment is derived from the delimiter row at draw time and never written back into the block or the op log.
The projection does not rewrite what the log holds; it only reads it.

**The draw is TUI-only for now.**
Per-client coverage is recorded as `outl_shortcuts::Capability::MarkdownTable`: `Full` on the TUI, `Missing` on desktop and mobile, each carrying a nudge that says the rows are still saved in full.
The grid bypasses `push_wrapped` (fixed columns want per-cell truncation, not word wrap) and uses only existing theme tokens (`theme.heading`, `theme.dim`, `theme.hint`, `theme.bullet`, `theme.foreground`).

## Why not the alternatives

**Run `comrak` and keep its table AST.**
`comrak` is already a dependency but wired for nothing, and its table node is a `Node` in the CommonMark tree — a shape the outline AST has no home for.
Bridging it means either teaching the outline a second block kind that the op log, sidecar, and matching all have to learn (invariant 1: every mutation routes through `Op`), or throwing the parse tree away and re-parsing at render — a second markdown implementation for one block type.
The recognition predicate here is ~40 lines and round-trips through the dialect that already exists.

**Store the table as structured rows in the op log.**
That is a new op payload for a projection concern.
Alignment is a display choice; the `.md` must stay 100% clean pipe text (invariant 2), and the block must round-trip through any CommonMark renderer.
Structuring it in the log buys nothing the pipe text doesn't already encode, and costs every consumer a migration.

**Refuse a malformed table instead of accepting it.**
A blank delimiter cell or a ragged row would fail `parse_table_block` and fall back to per-line recovery — splitting the table back into N blocks with N warnings, which is precisely the loss this module exists to prevent.
So a recognised-but-ragged table is accepted, padded, and truncated to the header's column count: it survives as verbatim text.

**Give a mid-block or indented table its own node in the tree.**
Rejected, and still rejected.
A table living inside a block stays that block's continuation text — it never becomes a node with its own id, fold, or op.
The TUI *does* now draw such a table as a grid (see Scope → mid-block), but purely at render time: `outl_md::table_run_len` finds the run inside the block's stored lines, the grid is projected over it, and the block's identity is untouched.
The rejected alternative was making the grid's existence a structural fact; a projection over lines the parser already keeps is not that.

## The opposite direction

**Storage does not change, so the mirror-image data-loss failure is structurally unavailable here.**
RFC 0210 was caused by a fix that let a projection overwrite bytes the log never saw.
This change never writes to storage: parse collapses rows into one block's text *without altering any of them* (`consume_table_block` trims leading indent and joins with `\n`, and the renderer re-emits the identical bytes), and render only reads.
`a_collapsed_table_is_a_render_fixpoint` pins that `render → parse` reproduces the input, and `a_table_stores_its_delimiter_verbatim` pins that the delimiter row — the alignment hint that only ever mattered for display — is stored unchanged.

**The refusal path is "don't collapse", not "lose the line".**
When `looks_like_table_start` / `parse_table_block` say no — a paragraph whose second line carries a `|`, a table glued to a bullet, a one-line table — the parser takes the *existing* recovery arm: the line becomes a verbatim block with a warning, exactly as it did before this PR.
`a_paragraph_with_a_pipe_is_not_mistaken_for_a_table` pins that no prose is hijacked into a table, so the new recognition cannot silently re-shape a paragraph into a grid the user didn't write.
What the change *does* make newly reachable is a top-level table rendering with column alignment on the TUI; on a GUI client the same table renders as its raw rows, and `Capability::MarkdownTable` records that gap so no user mistakes raw rows for lost content.

## How it cannot regress

1. **The invariants.**
   Invariant 2 (clean `.md`) and invariant 8 (never advance a projection over content the log lacks) are stated in the root `CLAUDE.md`; the byte-verbatim, render-only claim lives in the `outl_md::table` module doc and the `outl-md/CLAUDE.md` "Pipe tables" bullet.
   Invariant 12 (a capability without a per-client verdict is a gap nobody recorded) is enforced structurally: `MarkdownTable` is a variant in the exhaustive `capability_support` match, so a client cannot silently not-draw it.

2. **The tests.** Each fails if the gate is re-simplified to a `contains("|")` heuristic or the collapse is dropped:

   - Parse (outl-md `src/parse/tests.rs`): `a_top_level_table_collapses_to_one_block_without_warning`, `a_table_absorbs_its_rows_and_stops_at_the_next_block`, `a_collapsed_table_is_a_render_fixpoint`, `a_paragraph_with_a_pipe_is_not_mistaken_for_a_table`, `a_table_stores_its_delimiter_verbatim`.
   - Column model (outl-md `src/table.rs`): `parse_reads_header_delimiter_and_alignment`, `parse_pads_short_rows_and_truncates_long_ones`, `parse_rejects_anything_that_is_not_a_whole_table`, `center_alignment_needs_colons_both_sides`; `run_len_*` pin the mid-block/nested recognizer (`table_run_len`) to the same triad, stop it at prose, require a data row, and keep it indent-agnostic.
   - Render (outl-tui `src/view/table.rs`): `a_pretty_block_that_is_a_table_renders_as_a_grid` and `an_edited_table_block_keeps_its_raw_source` drive the real `emit_block_lines` (so a regression that stops the pretty branch from delegating, or that draws the grid over a block under the cursor, fails); `every_row_shares_one_display_width` and `rule_marks_declared_alignment` pin the geometry; `a_mid_prose_table_renders_as_a_grid_between_its_prose`, `a_mid_prose_grid_carries_no_bullet` and `a_nested_mid_prose_grid_draws_the_parents_rails` pin the mid-block run: prose kept either side, no second bullet, the parent's rails drawn once. The `[tui] table_style = "box"` frame is pinned by `a_boxed_standalone_table_frames_top_and_bottom`, `a_boxed_grid_shares_one_display_width_across_every_row` and `the_box_border_aligns_with_the_column_rules` (the `┬`/`┴` tees land on the interior gutters), and `a_standalone_table_reaches_the_outline_boxed_when_configured` / `a_mid_prose_table_is_never_boxed` pin that only a standalone run boxes — a mid-prose grid never grows a wall onto the carrying block's content column.
   - Corpus gate (outl-md `tests/corpus/gfm_table.md` and `gfm_table_mid_block.md`): the three corpus properties run over a real table, including the `render → parse` fixpoint; the mid-block fixture pins that a table living inside a block's text (prose around it, and one nested a level deeper) round-trips through disk unchanged and is accounted for by the unlogged-content check.
   - Capability parity (outl-shortcuts): `the_parity_doc_matches_the_code` pins `docs/client-parity.md` against the verdicts, so a client that starts (or stops) drawing the grid must move the row.

## Scope

- **Desktop and mobile renderers.**
  [#329](https://github.com/outlmd/outl/issues/329) stays open until both GUI clients draw the grid; until then `Capability::MarkdownTable` reads `Missing` on both, and the raw pipe rows are the honest fallback.
  When a client grows a renderer, it wraps `outl_md::parse_table_block` (one owner of the column model) rather than writing a TS table parser, and flips its column to `Full`.

- **Mid-block / indented tables — recognized at render, never collapsed.**
  A table sitting inside a block (mid-prose, or under a bullet) is still *not* its own node: it stays the block's continuation text, recovered exactly as before.
  What shipped is render-only: `emit_block_lines` walks the block's lines, and where `outl_md::table_run_len` reports a run, it projects a grid over those rows and leaves the surrounding lines as prose.
  The run is drawn under the block's content column (`emit_table_lines` takes `carries_bullet: false` so a mid-prose grid borrows no bullet of its own).
  Storage is untouched and invariant 8 stays safe: those rows were already block continuation text that `content_lines_missing_from` accounts for.
  The one shape that stays literal is a table sharing a *single line* with other text — there is no row axis in the inline `Span` pipeline, so that line renders as ordinary prose.

- **The box frame is a TUI-only view preference, not new state.**
  `[tui] table_style = "open" | "box"` (`open` default) decides whether a *standalone* table is wrapped in a top border, side walls and a bottom border.
  It is pure display state like `theme.preset` and `[display] backlinks_order` (root invariant 7): read once at boot into `App::box_tables`, never an `Op`, never converges between devices, and the desktop ignores the section — a GUI draws the grid in JSX and has no notion of an ASCII frame.
  A mid-prose / nested run ignores the flag and stays open, so a wall never lands on the carrying block's content column.

- **Query-result tables (` ```query `) are a different thing.**
  [RFC 0280](0280-task-query-tables.md) covers tabular *query output*; it shares only the word "table" with this RFC, and neither owns the other.
