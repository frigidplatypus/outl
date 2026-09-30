/**
 * The aligned pipe-table grid, drawn from the `Table` structure
 * {@link parseTableBlock} recovers — the GUI mirror of the TUI's
 * `view/table.rs` drawer.
 *
 * One drawer shared by every Solid client (invariant: pure shared
 * rendering lives in `outl-frontend-shared`), theme tokens only
 * (invariant 13 — no hex literals). Column widths are allocated in
 * proportion to each column's widest cell measured in terminal cells
 * (East-Asian wide glyphs count 2), the same budget rule the TUI uses,
 * so a table that fits the pane in the TUI fits the pane here; the
 * allocation becomes a CSS percentage, and `table-fixed` truncates
 * overflow per cell instead of widening the page.
 */
import { For } from "solid-js";

import type { Align, Table } from "./table";

/** Framing of the grid — mirrors `outl_config::TableStyle`. */
export type TableStyle = "open" | "box";

/**
 * Display width of a cell in terminal cells: code points count 1,
 * East-Asian wide/fullwidth ranges count 2, combining marks count 0.
 * An approximation of `unicode-width` (the Rust side) covering the
 * ranges a table cell realistically hits.
 */
function displayWidth(text: string): number {
  let w = 0;
  for (const ch of text) {
    const cp = ch.codePointAt(0)!;
    if (cp === 0x200b || (cp >= 0x300 && cp <= 0x36f) || (cp >= 0x200b && cp <= 0x200f)) continue;
    if (
      (cp >= 0x1100 && cp <= 0x115f) ||
      (cp >= 0x2e80 && cp <= 0xa4cf && cp !== 0x303f) ||
      (cp >= 0xac00 && cp <= 0xd7a3) ||
      (cp >= 0xf900 && cp <= 0xfaff) ||
      (cp >= 0xfe30 && cp <= 0xfe6f) ||
      (cp >= 0xff00 && cp <= 0xff60) ||
      (cp >= 0xffe0 && cp <= 0xffe6) ||
      (cp >= 0x20000 && cp <= 0x3fffd)
    ) {
      w += 2;
    } else {
      w += 1;
    }
  }
  return w;
}

function alignClass(align: Align): string {
  if (align === "center") return "text-center";
  if (align === "right") return "text-right";
  return "text-left";
}

export interface TableGridProps {
  /** The recovered column grid. */
  table: Table;
  /** `box` frames the grid with a border; `open` rules the header only. */
  style: TableStyle;
}

/**
 * A read-only table view: clicking any cell is the caller's problem —
 * the owning row's `onClick` already enters edit mode, and this
 * component renders inside that row, so the event bubbles up.
 */
export function TableGrid(props: TableGridProps) {
  // Per-column weight = the widest cell in that column (header + data),
  // minimum 3 cells so a single-word column keeps a legible share.
  const weights = () =>
    props.table.header.map((head, col) => {
      let widest = Math.max(displayWidth(head), 3);
      for (const row of props.table.rows) {
        widest = Math.max(widest, displayWidth(row[col] ?? ""));
      }
      return widest;
    });

  // `table-fixed` + explicit percentage widths keep the grid inside the
  // block's column: overflow truncates per cell (the TUI truncates at
  // its pane budget; a proportional font truncates at the CSS budget).
  const total = () => Math.max(1, weights().reduce((a, b) => a + b, 0));
  const widthOf = (col: number) => `${((weights()[col] / total()) * 100).toFixed(2)}%`;

  const box = () => props.style === "box";

  return (
    <div
      class={`my-1 w-full overflow-hidden ${
        box() ? "rounded border border-(--color-outl-border)" : ""
      }`}
    >
      <table class="table-fixed w-full border-collapse">
        <colgroup>
          <For each={props.table.header}>
            {(_, col) => <col style={{ width: widthOf(col()) }} />}
          </For>
        </colgroup>
        <thead>
          <tr class="border-b border-(--color-outl-border) font-medium">
            <For each={props.table.header}>
              {(cell, col) => (
                <th
                  class={`truncate px-2 py-0.5 text-left font-medium ${alignClass(props.table.alignments[col()])}`}
                >
                  {cell}
                </th>
              )}
            </For>
          </tr>
        </thead>
        <tbody>
          <For each={props.table.rows}>
            {(row) => (
              <tr>
                <For each={row}>
                  {(cell, col) => (
                    <td
                      class={`truncate px-2 py-0.5 ${alignClass(props.table.alignments[col()])}`}
                    >
                      {cell}
                    </td>
                  )}
                </For>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </div>
  );
}
