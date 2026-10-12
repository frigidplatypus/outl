import { describe, expect, it } from "vitest";

import type { PageMeta } from "@outl/shared/api/types";

import type { Binding } from "./api";
import { CHORDLESS_ACTIONS, paletteActions, paletteResults, sectionOf } from "./palette-results";

const page = (slug: string): PageMeta => ({ id: slug, slug, title: slug, kind: "page" });
const cmdK = [{ mods: 8, key: { kind: "Char", value: "k" } }] as Binding["chord"];
const ctrlK = [{ mods: 1, key: { kind: "Char", value: "k" } }] as Binding["chord"];

const bindings: Binding[] = [
  { chord: cmdK, mode: "global", action: { kind: "OpenToday" }, description: "Open today's journal" },
  { chord: ctrlK, mode: "global", action: { kind: "OpenToday" }, description: "Ctrl twin" },
  { chord: cmdK, mode: "normal", action: { kind: "ToggleSidebar" }, description: "Toggle sidebar" },
  { chord: cmdK, mode: "insert", action: { kind: "WrapBold" }, description: "Bold" },
  { chord: cmdK, mode: "global", action: { kind: "OpenPicker" }, description: "Open quick switcher" },
  { chord: cmdK, mode: "global", action: { kind: "Quit" }, description: "Quit" },
] as Binding[];

const actions = paletteActions(bindings, (k) => k !== "Quit" && k !== "ExportMarkdown", true);

describe("paletteActions", () => {
  it("keeps one row per handled global or normal action, first description wins", () => {
    expect(actions.map((a) => [a.action, a.label])).toEqual([
      ["OpenToday", "Open today's journal"],
      ["ToggleSidebar", "Toggle sidebar"],
    ]);
  });

  it("shows the Cmd chord on macOS and the Ctrl twin elsewhere", () => {
    expect(actions[0].chord).toEqual(cmdK);
    const other = paletteActions(bindings, (k) => k !== "Quit" && k !== "ExportMarkdown", false);
    expect(other[0].chord).toEqual(ctrlK);
    expect(other[0].label).toBe("Open today's journal");
  });
});

describe("paletteResults", () => {
  const blocks = [{ handle: "blk-1", id: "n1", text: "today notes", source_slug: "a" }];

  it("lists only pages for an empty query", () => {
    const r = paletteResults({ query: "", pages: [page("a")], blocks, actions });
    expect(r.map(sectionOf)).toEqual(["Pages"]);
  });

  it("orders pages, blocks, then matching actions", () => {
    const r = paletteResults({ query: "tod", pages: [page("today")], blocks, actions });
    expect(r.map(sectionOf)).toEqual(["Pages", "Blocks", "Actions"]);
  });

  it("holds blocks back for a one-letter query", () => {
    const r = paletteResults({ query: "t", pages: [], blocks, actions });
    expect(r.some((i) => i.kind === "block")).toBe(false);
  });

  it("narrows to actions after >", () => {
    const r = paletteResults({ query: "> side", pages: [page("side")], blocks, actions });
    expect(r).toEqual([actions[1]]);
  });

  it("shows every action for a bare >", () => {
    expect(paletteResults({ query: ">", pages: [], blocks, actions })).toHaveLength(2);
  });
});

describe("tasks", () => {
  const hit = (id: string, text: string) => ({ handle: id, id, text, source_slug: "p" });
  const blocks = [hit("b1", "notes on milk"), hit("t1", "TODO buy milk"), hit("t2", "[x] milk run")];

  it("splits task hits into their own section, before blocks", () => {
    const r = paletteResults({ query: "milk", pages: [], blocks, actions: [] });
    expect(r.map(sectionOf)).toEqual(["Tasks", "Tasks", "Blocks"]);
    expect(r[0]).toMatchObject({ kind: "task", state: "todo" });
    expect(r[1]).toMatchObject({ kind: "task", state: "done" });
  });

  it("caps tasks and blocks independently", () => {
    const many = Array.from({ length: 9 }, (_, i) => hit(`t${i}`, `TODO milk ${i}`));
    const r = paletteResults({ query: "milk", pages: [], blocks: [...many, hit("b", "milk")], actions: [] });
    expect(r.filter((i) => i.kind === "task")).toHaveLength(5);
    expect(r.filter((i) => i.kind === "block")).toHaveLength(1);
  });
});

describe("chordless actions", () => {
  it("lists Export as Markdown by name once it has a handler", () => {
    const rows = paletteActions([], (k) => k === "ExportMarkdown", true);
    expect(rows).toEqual([{ kind: "action", action: "ExportMarkdown", label: "Export as Markdown", chord: null }]);
    const r = paletteResults({ query: "> export", pages: [], blocks: [], actions: rows });
    expect(r).toHaveLength(1);
  });

  it("stays out when the client has no handler", () => {
    expect(paletteActions([], () => false, true)).toEqual([]);
    expect(CHORDLESS_ACTIONS.every((c) => c.label.length > 0)).toBe(true);
  });
});
