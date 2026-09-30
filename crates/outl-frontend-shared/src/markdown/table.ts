/**
 * Pipe tables in the outl dialect — the TypeScript mirror of
 * `outl-md`'s `table.rs` column model.
 *
 * The Rust module is the owner: it decides which `.md` lines collapse into
 * one block, and every client receives the collapsed block text (pipe rows
 * joined by `\n`) over the wire already. What the Rust side cannot give a
 * GUI client is the *grid* — `Table` lives in a crate the TypeScript side
 * cannot link. This file re-derives it from the block text, so the parse
 * rules below are a deliberate line-for-line port of `table.rs`:
 *
 * - recognition: {@link isTableRow}, {@link isDelimiterRow},
 *   {@link tableRunLen}
 * - structure: {@link Table}, {@link parseTableBlock}
 *
 * The two parsers are pinned against each other by a shared corpus
 * (`outl-md/tests/corpus/table_grid.json`): a Rust test regenerates the
 * expected grids from `table.rs`, and the vitest suite here asserts the
 * port agrees. Recognition drift is a bug users see as a table silently
 * rendering as prose on one client only.
 *
 * Deliberately not a full GFM table parser — same rules, same reasons as
 * the Rust doc-comment: ragged rows are padded/truncated to the header
 * width, and only `\|` / `\\` unescape.
 */

/** Horizontal alignment of a table column, read from its delimiter cell. */
export type Align = "left" | "center" | "right";

/** A pipe table recovered from a block's stored text. */
export interface Table {
  /** One alignment per column, taken from the delimiter row. */
  alignments: Align[];
  /** The header row's cells. */
  header: string[];
  /** The data rows, in source order. */
  rows: string[][];
}

/** A row of a pipe table: non-empty once trimmed, and carrying a `|`. */
export function isTableRow(line: string): boolean {
  const t = line.trim();
  return t.length > 0 && t.includes("|");
}

/**
 * One cell of a delimiter row: optional surrounding spaces and colons
 * around one or more `-`.
 */
function isDelimiterCell(cell: string): boolean {
  const core = cell.trim().replace(/^:+/, "").replace(/:+$/, "");
  return core.length > 0 && /^-+$/.test(core);
}

/**
 * The table's alignment row (`|---|:--:|--:|`). Strict on cells: once the
 * leading and trailing pipes are stripped, every remaining cell must be a
 * delimiter cell, so a header row (`|a|b|`) never reads as a delimiter.
 */
export function isDelimiterRow(line: string): boolean {
  const t = line.trim();
  if (!t.includes("|")) return false;
  let inner = t.startsWith("|") ? t.slice(1) : t;
  inner = inner.endsWith("|") ? inner.slice(0, -1) : inner;
  if (inner.trim().length === 0) return false;
  return inner.split("|").every(isDelimiterCell);
}

/**
 * Split a header or data row into cells, honouring `\|` as a literal pipe
 * and `\\` as a literal backslash (both unescaped), and trimming each cell.
 * Leading and trailing pipes are the row's fences, not cells. Iterates
 * code points, matching Rust's `chars()`.
 */
export function splitRow(line: string): string[] {
  const t = line.trim();
  const inner = t.startsWith("|") ? t.slice(1) : t;
  const body = inner.endsWith("|") ? inner.slice(0, -1) : inner;
  const cells: string[] = [];
  let cur = "";
  const chars = Array.from(body);
  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i];
    if (ch === "\\" && (chars[i + 1] === "\\" || chars[i + 1] === "|")) {
      cur += chars[i + 1];
      i++;
    } else if (ch === "|") {
      cells.push(cur.trim());
      cur = "";
    } else {
      cur += ch;
    }
  }
  cells.push(cur.trim());
  return cells;
}

/** Alignment a delimiter cell declares. */
function cellAlign(cell: string): Align {
  const c = cell.trim();
  const lead = c.startsWith(":");
  const trail = c.endsWith(":");
  if (lead && trail) return "center";
  if (!lead && trail) return "right";
  return "left";
}

/**
 * Length of the pipe table starting at `lines[start]`, or `null` if no
 * table starts there — the render-time recognizer for a table run *inside*
 * a block (mid-prose, or nested under a bullet). Mirrors `table_run_len`:
 * header + delimiter + at least one data row, absorbing every immediately
 * following table row.
 */
export function tableRunLen(lines: string[], start: number): number | null {
  const header = lines[start];
  if (header === undefined || !isTableRow(header) || isDelimiterRow(header)) {
    return null;
  }
  const delim = lines[start + 1];
  if (delim === undefined || !isDelimiterRow(delim)) return null;
  let end = start + 2;
  while (end < lines.length && isTableRow(lines[end])) end++;
  // Header + delimiter is not yet a table: it needs one data row.
  if (end < start + 3) return null;
  return end - start;
}

/**
 * Recover the column grid from a block's stored `text`, or `null` unless
 * the text is a whole table: three or more lines whose first is a header
 * row, second a delimiter row, and every line after a data row. Short rows
 * are padded and long rows truncated to the header's width; cells arrive
 * unescaped and trimmed.
 */
export function parseTableBlock(text: string): Table | null {
  const lines = text.split("\n");
  // Rust's `.lines()` on text with no trailing newline matches split("\n")
  // for stored block text (never trailing-newline); guard the empty tail
  // split produces if a renderer ever passes one through.
  if (lines.length > 1 && lines[lines.length - 1] === "") lines.pop();
  if (lines.length < 3) return null;
  const header = lines[0];
  if (!isTableRow(header) || isDelimiterRow(header)) return null;
  if (!isDelimiterRow(lines[1])) return null;
  if (!lines.slice(2).every(isTableRow)) return null;

  const headerCells = splitRow(header);
  const delimCells = splitRow(lines[1]);
  const width = headerCells.length;

  const alignments: Align[] = [];
  for (let idx = 0; idx < width; idx++) {
    const cell = delimCells[idx];
    alignments.push(cell !== undefined && cell.length > 0 ? cellAlign(cell) : "left");
  }

  const rows = lines.slice(2).map((line) => {
    const cells = splitRow(line);
    if (cells.length > width) cells.length = width;
    while (cells.length < width) cells.push("");
    return cells;
  });

  return { alignments, header: headerCells, rows };
}
