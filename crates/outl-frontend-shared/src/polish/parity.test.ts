/**
 * Visual parity guards: the few class choices desktop and mobile must
 * make the same way. Read as text, because rendering either row needs
 * its whole client.
 */
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const crates = resolve(__dirname, "../../..");
const read = (p: string) => readFileSync(resolve(crates, p), "utf8");

describe("desktop and mobile rows", () => {
  it("block text and its textarea share one line height token", () => {
    for (const f of [
      "outl-desktop/src/components/BlockRow.tsx",
      "outl-mobile/src/components/BlockBody.tsx",
      "outl-mobile/src/components/BlockTextarea.tsx",
    ]) {
      expect(read(f), f).toContain("leading-(--outl-block-leading)");
    }
  });

  it("task markers use the theme's task colours, not the accent", () => {
    const mobile = read("outl-mobile/src/components/BlockMarkers.tsx");
    const desktop = read("outl-desktop/src/components/BlockRow.tsx");
    for (const src of [mobile, desktop]) {
      expect(src).toContain("--color-outl-todo-open-fg");
      expect(src).toContain("--color-outl-todo-done-fg");
    }
  });
});
