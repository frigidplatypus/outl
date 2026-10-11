import { describe, expect, it } from "vitest";

import type { PageMeta } from "@outl/shared/api/types";

import type { Binding } from "./api";
import { paletteActions, paletteResults, sectionOf } from "./palette-results";

const page = (slug: string): PageMeta => ({ id: slug, slug, title: slug, kind: "page" });
const cmdK = [{ mods: 8, key: { kind: "Char", value: "k" } }] as Binding["chord"];

const bindings: Binding[] = [
  { chord: cmdK, mode: "global", action: { kind: "OpenToday" }, description: "Open today's journal" },
  { chord: cmdK, mode: "global", action: { kind: "OpenToday" }, description: "Ctrl twin" },
  { chord: cmdK, mode: "normal", action: { kind: "ToggleSidebar" }, description: "Toggle sidebar" },
  { chord: cmdK, mode: "insert", action: { kind: "WrapBold" }, description: "Bold" },
  { chord: cmdK, mode: "global", action: { kind: "OpenPicker" }, description: "Open quick switcher" },
  { chord: cmdK, mode: "global", action: { kind: "Quit" }, description: "Quit" },
] as Binding[];

const actions = paletteActions(bindings, (k) => k !== "Quit");

describe("paletteActions", () => {
  it("keeps one row per handled global or normal action, first description wins", () => {
    expect(actions.map((a) => [a.action, a.label])).toEqual([
      ["OpenToday", "Open today's journal"],
      ["ToggleSidebar", "Toggle sidebar"],
    ]);
  });
});

describe("paletteResults", () => {
  const blocks = [{ handle: "blk-1", text: "today notes", source_slug: "a" }];

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
