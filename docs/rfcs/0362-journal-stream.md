# RFC 0362: The journal is a stream of days, and each day keeps its own undo

| | |
|---|---|
| **Status** | Draft |
| **Issue** | none yet. The number is the PR that carries this RFC, [#362](https://github.com/outlmd/outl/pull/362), which ships no stream code |
| **PR** | [#362](https://github.com/outlmd/outl/pull/362) |
| **Date** | 2026-10-11 |
| **Reference doc** | none yet; `UX.md` principle 5 (view state is local) and `crates/outl-desktop/CLAUDE.md` once built |
| **Invariant** | root `CLAUDE.md` invariant 7 (state that must converge goes through the op log), by staying out of it: everything here is local view state |
| **Guarded by** | none yet, this is a Draft. The tests the implementation owes are listed under "How it cannot regress" |

## Why

A journal is read across days, and outl shows one day at a time.
To see what you wrote yesterday next to what you are writing today you press `Cmd+[`, read, press `Cmd+]`, and the context you wanted side by side is gone.
On mobile it is the same with the chevrons.
The journal is the page people open most, and it is the one page where the neighbouring pages are always relevant.

The desktop is built around that single page on purpose, and it shows:

- `appState.page` and `appState.outline` in `crates/outl-desktop/src/lib/store.ts` hold one page.
- `undoPage(pageId)` / `redoPage(pageId)` are scoped to one page.
- `selectedBlockId`, `editingBlockId` and the visual anchor assume they live in that one outline, and the vim motions walk it.
- Backlinks are fetched lazily for that one slug after the outline paints (RFC 0169's lazy policy).

So "show the previous days under today" is not a layout change. It changes what owns focus, undo, selection and scroll.

## What we chose

A **journal stream** on the desktop: today, then the previous N days that have content, in one scroll, each day rendered by the same outline component it uses today.

- **One owner for the stream:** a `JournalStream` component and a `journalStream` slice of the store holding an ordered list of day views, each `{ page, outline }`. `appState.page` stays the *focused* day, so everything that reads it today keeps working.
- **Focus moves the page.** Selecting or editing a block in a day makes that day the focused page. `Cmd+[` / `Cmd+]` scroll to the neighbouring day instead of replacing the view.
- **Undo stays per day.** `Cmd+Z` undoes on the focused day, because that is what `undo_page` already does and what a user editing yesterday expects. There is no stream-wide undo.
- **Vim motions stop at a day's edge.** `j` on the last block of today does not jump into yesterday. Crossing days is `Cmd+[` / `Cmd+]`, as now.
- **Days load lazily.** The stream asks for the next day when the previous one scrolls into view. Empty days are skipped, which needs a backend list of journal slugs with content (a new read-only command, no Op).
- **Backlinks per day,** collapsed, fetched when a day becomes focused.
- **Mobile** keeps one day per screen and gets swipe between days instead. A stream on a phone fights the keyboard and the swipe-to-indent gestures in `UX.md`.

## Why not the alternatives

**Render the stream as one virtual page.** Merging the days into one outline would give one undo stack and free vim motions across days, but a block's page would become ambiguous at every boundary, and an edit could land in the wrong `.md`. The per-page model is what keeps every write going to one file.

**A stream-wide undo.** It would need either a new op that groups ops across pages or a frontend log of which page each undo belongs to. The first is an op log change for a view convenience. The second reorders undo across pages behind the user's back. Per-day undo is what each day already has.

**Copy Reflect's continuous editor.** Reflect's daily stream works because its editor is one ProseMirror document per note and the stream is notes stacked. That is close to what this RFC does, minus the engine. Switching the engine would break the block model, the vim mode and markdown roundtrip.

**Do nothing, add a "previous day" peek panel.** Cheaper, but it is a second rendering of a page with its own rules for editing, and the next request is "let me edit in the peek".

## The opposite direction

What gets worse:

- **Memory and first paint.** Several outlines are mounted instead of one. Days must load lazily and unmount when far off screen, or a long scroll on a big workspace reproduces the boot problems RFC 0128 fixed.
- **Where the keyboard goes is less obvious.** With one page, every key acted on it. With a stream, a key acts on the focused day, which the user has to see. The focused day needs a visible cue.
- **Undo surprises.** Edit today, click into yesterday, press `Cmd+Z`: nothing happens to today. That is correct per day and still surprising. The cue above has to make it legible.
- **Sync arriving for an unfocused day** has to re-render that day without stealing focus or scroll. Today a peer change refreshes the one page you look at.

What it does not permit: nothing here writes anything that didn't write before. No new Op, no sidecar field, no markdown change. Which day is focused and how far you scrolled never leaves the device (`UX.md` principle 5).

## How it cannot regress

Owed by the implementation PR, none exist yet:

1. A rule in `crates/outl-desktop/CLAUDE.md`: a stream day is a page, every mutation goes through that day's `pageId`, and undo never crosses days.
2. Tests:
   - `an_edit_in_a_stream_day_writes_to_that_days_page`, so a boundary edit can never land in a neighbour.
   - `undo_in_the_stream_only_touches_the_focused_day`.
   - `vim_j_on_the_last_block_of_a_day_stays_in_that_day`.
   - `a_peer_change_to_an_unfocused_day_keeps_focus_and_scroll`.
   - `days_far_off_screen_unmount`, with a block count bound so memory cannot grow with scroll length.

## Trust you can see

The same PR makes the engine's guarantees visible.
Page history renders each edit as a word diff and offers a restore where one is safe: an edit's previous text goes back through `edit_block`, a deleted block's text comes back through `create_block`.
Each restore is new ops, never a rewind of the log, so it syncs and undoes like any edit; moves and property changes get no button because their "before" depends on neighbours.
The sync settings show what the op log holds from each actor (`outl_actions::device_activity`), labelled honestly, since an actor id is not a peer id.
`export_markdown` copies the clean `.md` files and assets into a new folder next to the workspace.
A block shows a short "saved" dot once `edit_block` resolves.
Not done here: a list of merged concurrent edits with an undo. The CRDT converges without recording that two edits raced, so showing it needs the engine to detect concurrency first; that is a follow-up with its own design.

## Polish you can feel

Motion is short and only ever opacity or transform: children slide in when a block unfolds (never on page load), a drop target eases into its ring, and the sync dot pulses once when a pass ends. Under `prefers-reduced-motion` all of it is off and the end state shows at once.
Reading gets one rhythm on both clients (`outl-reading`: 1.6 line height, kerning and ligatures, a little air between blocks). Body text uses `fg`, which clears 4.5:1 on all ten presets, and a test pins that.
Focus mode fades the chrome and dims every row but the one being written, with a 68ch measure on desktop. It is a device preference, like scroll position. It ships as a button in both capsules; a keyboard chord needs an `Action`, which waits for `outl-shortcuts`' baselined files to be split.
A new user gets three tips (blocks, links, daily) on an empty or blank page, in the same words on both clients, dismissed once per device.
Known differences left on purpose: refs render as pills on mobile and underlined inline text on desktop (`MarkdownInline`'s `variant`, chosen for touch versus long reading), and the TODO glyph is a circle on mobile and a square on desktop. Changing either is a design call, not an alignment fix.

## Speed and flow

One box finds everything. The desktop palette (`Cmd/Ctrl+P`, or `Cmd/Ctrl+K` on `>` for actions) lists pages, then tasks, then blocks, then actions. A task is a block hit whose text carries a task marker, read by the shared `taskState`, the same reading the bullet toggle uses, so "task" means one thing everywhere.
Rare actions get no chord but are still found by name: `CHORDLESS_ACTIONS` lists them, starting with "Export as Markdown", which runs the same `export_markdown` the Settings card does.
Quick capture is one line into the end of today's journal through `create_block`, so it is an op like any typed block. Three doors open the box: the `QuickCapture` action (`Cmd/Ctrl+Shift+Space`), the same chord registered with the OS through `tauri-plugin-global-shortcut` so it works with the window in the background, and `outl://capture`. On iOS the link opens the existing capture sheet, which is step 1 of RFC 0363. A chord another app already owns is logged and skipped; the other doors still work.
`QuickCapture` and `ExportMarkdown` are new `Action`s with a verdict for all three clients in `outl_shortcuts::support`; the TUI is `Missing` for both, with the reason in `support/why.rs`.
Not done here, on purpose: undo for sync and plugin actions, and a full keyboard map of the desktop chrome. Undo today is a per-page snapshot stack of this device's own edits. Undoing a peer's edit means emitting compensating ops for changes this device never made, and a plugin's writes need to be attributed before they can be stepped back; both are engine questions for their own design. Until then, page history's restore is the way back from a sync, and it is already new ops. Keyboard reach for the sidebar, panels and capsules needs focus management across components and lands separately.

## Scope

Not covered: mobile day swipe (a separate, smaller change with no RFC need), the iOS quick capture outside the app beyond the `outl://capture` link (it writes from another process, which is an op log and actor question for RFC 0363), and any change to how journal pages are created or named (RFC 0107).
