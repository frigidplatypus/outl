/**
 * What the Cmd+K / Cmd+P palette lists for a query. Pure, so the
 * ranking is testable without a DOM or a backend.
 *
 * One box finds three kinds of thing, in this order:
 *
 * - **pages**, ranked by the backend's `search_pages` (exact, prefix,
 *   substring), so the palette and `[[` autocomplete agree;
 * - **tasks**, the `search_blocks` hits that carry a task marker
 *   (`TODO`, `DOING`, `DONE`, or a checkbox), read with the same
 *   `taskState` the bullet toggle uses, listed before plain blocks
 *   because "find that todo" is the commonest reason to search;
 * - **blocks**, the rest of the hits, once the query is long enough to
 *   mean something (a single letter matches half the workspace);
 * - **actions**, from the shortcut catalog, each with its chord, so the
 *   palette doubles as the place you learn the keys. Actions with no
 *   chord at all ({@link CHORDLESS_ACTIONS}) are listed by name only.
 *
 * A leading `>` narrows the list to actions, the convention most
 * editors use. That is what `:` and Cmd+K in Normal mode open with.
 */
import type { BlockHit } from "@outl/shared/api/commands";
import type { PageMeta } from "@outl/shared/api/types";
import { taskState } from "@outl/shared/outline";

import { MOD_META, type Action, type Binding, type Chord } from "./api";

export type PaletteItem =
  | { kind: "page"; page: PageMeta }
  | { kind: "block"; hit: BlockHit }
  | { kind: "task"; hit: BlockHit; state: "todo" | "doing" | "done" }
  | { kind: "action"; action: Action["kind"]; label: string; chord: Chord[] | null };

export const COMMAND_PREFIX = ">";
/** Blocks start showing at this many characters. */
export const BLOCK_QUERY_MIN = 2;

const LIMITS = { pages: 8, tasks: 5, blocks: 6, actions: 5 } as const;

/**
 * Actions the palette lists although no chord reaches them: things you
 * do rarely enough that a key would be a waste, but should still find
 * by typing their name. Each still owes a row in `outl_shortcuts::support`.
 */
export const CHORDLESS_ACTIONS: ReadonlyArray<{ action: Action["kind"]; label: string }> = [
  { action: "ExportMarkdown", label: "Export as Markdown" },
];

/**
 * Actions that make no sense as a palette row: the palette itself, and
 * anything that only means something mid-chord or mid-edit (cursor
 * motion, text objects). Those have no `global` or `normal` binding,
 * which is the filter `paletteActions` applies; this list is only for
 * the two that do.
 */
const NOT_LISTED = new Set<Action["kind"]>(["OpenPicker", "OpenCommandPalette"]);

/**
 * One row per action that has a handler and a `global` or `normal`
 * binding, labelled with the first binding's description. The catalog
 * lists a Cmd chord before its Ctrl twin, so off macOS a later binding
 * without Cmd replaces the chord: the row teaches a key that works here.
 */
export function paletteActions(
  bindings: Binding[],
  handled: (kind: Action["kind"]) => boolean,
  mac: boolean = isMac(),
): Extract<PaletteItem, { kind: "action" }>[] {
  const seen = new Map<Action["kind"], Extract<PaletteItem, { kind: "action" }>>();
  const usesMeta = (c: Chord[]) => c.some((k) => (k.mods & MOD_META) !== 0);
  for (const b of bindings) {
    const kind = b.action.kind;
    if (b.mode !== "global" && b.mode !== "normal") continue;
    if (NOT_LISTED.has(kind) || !handled(kind)) continue;
    const prev = seen.get(kind);
    if (!prev) {
      seen.set(kind, { kind: "action", action: kind, label: b.description, chord: b.chord });
    } else if (!mac && prev.chord && usesMeta(prev.chord) && !usesMeta(b.chord)) {
      prev.chord = b.chord;
    }
  }
  for (const c of CHORDLESS_ACTIONS) {
    if (!seen.has(c.action) && handled(c.action)) {
      seen.set(c.action, { kind: "action", action: c.action, label: c.label, chord: null });
    }
  }
  return [...seen.values()];
}

function matches(label: string, q: string): boolean {
  return label.toLowerCase().includes(q);
}

export function paletteResults(input: {
  query: string;
  pages: PageMeta[];
  blocks: BlockHit[];
  actions: Extract<PaletteItem, { kind: "action" }>[];
}): PaletteItem[] {
  const raw = input.query.trim();
  if (raw.startsWith(COMMAND_PREFIX)) {
    const q = raw.slice(COMMAND_PREFIX.length).trim().toLowerCase();
    return input.actions.filter((a) => matches(a.label, q));
  }
  const q = raw.toLowerCase();
  const pages: PaletteItem[] = input.pages
    .slice(0, LIMITS.pages)
    .map((page) => ({ kind: "page", page }));
  if (!q) return pages;
  const tasks: PaletteItem[] = [];
  const blocks: PaletteItem[] = [];
  if (q.length >= BLOCK_QUERY_MIN) {
    for (const hit of input.blocks) {
      const state = taskState(hit.text);
      if (state && tasks.length < LIMITS.tasks) tasks.push({ kind: "task", hit, state });
      else if (!state && blocks.length < LIMITS.blocks) blocks.push({ kind: "block", hit });
    }
  }
  const actions = input.actions.filter((a) => matches(a.label, q)).slice(0, LIMITS.actions);
  return [...pages, ...tasks, ...blocks, ...actions];
}

/** Section heading for an item, used to draw a label when it changes. */
export function sectionOf(item: PaletteItem): string {
  const names = { page: "Pages", task: "Tasks", block: "Blocks", action: "Actions" } as const;
  return names[item.kind];
}

/** Whether the webview runs on macOS, where Cmd is the platform modifier. */
export function isMac(): boolean {
  return typeof navigator !== "undefined" && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
}
