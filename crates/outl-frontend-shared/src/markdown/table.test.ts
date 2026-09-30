/**
 * The TypeScript port of the pipe-table parser, pinned against the
 * same fixture `outl-md/tests/table_corpus.rs` pins the Rust side
 * against. Both suites read `outl-md/tests/corpus/table_grid.json`;
 * when either parser changes intentionally, the Rust test
 * regenerates the fixture
 * (`OUTL_UPDATE_TABLE_FIXTURE=1 cargo test -p outl-md --test
 * table_corpus`) and this suite fails until the port follows — the
 * GUI grid renderer (`TableGrid`) must see the grid the TUI sees.
 */
import { readFileSync } from "node:fs";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

import { describe, expect, it } from "vitest";

import { parseTableBlock, type Table } from "./table";

interface Case {
  name: string;
  text: string;
  expected: Table | null;
}

const here = dirname(fileURLToPath(import.meta.url));
const cases: Case[] = JSON.parse(
  readFileSync(
    join(here, "../../../outl-md/tests/corpus/table_grid.json"),
    "utf8",
  ),
);

describe("parseTableBlock against the shared corpus", () => {
  it("covers every case the Rust suite pins", () => {
    expect(cases.length).toBeGreaterThan(10);
  });

  it.each(cases.map((c) => [c.name, c] as const))("%s", (_name, c) => {
    expect(parseTableBlock(c.text)).toEqual(c.expected);
  });
});
