/**
 * ATX header rendering helpers for the desktop outline — **desktop
 * only**, mirroring the TUI's header chrome in
 * `crates/outl-tui/src/view/outline.rs` + `icons.rs`.
 *
 * The backend is the single owner of "is this a header": it derives
 * `BlockNode.header_level` via `outl_md::view::header_level` (the same
 * function the TUI calls directly). This file only knows how to *draw*
 * an already-identified header — the marker stripping below is the
 * wire-prefix counterpart of `@outl/shared/markdown`'s
 * `stripQuoteFromTokens`, because the backend tokenizes the raw block
 * text (so `# Section` ships as a single `Plain { value: "# Section" }`
 * token; the `# ` chrome is block-level, not inline).
 *
 * Mobile ignores `header_level` entirely (additive DTO field), so this
 * stays the desktop's, not `@outl/shared`'s.
 */

/** Header-level glyphs, index `level - 1`. Mirror of
 *  `outl-tui/src/icons.rs` `HEADER_1..6` — the Material Design
 *  `format_header_N` icons (U+F026B–U+F0270). A const can't cross the
 *  Rust/TS boundary, so the two tables are edited together (same rule
 *  as the `property_glyph` parity documented in
 *  `crates/outl-frontend-shared/CLAUDE.md`). */
export const HEADER_GLYPHS = [
  "\u{f026b}", // H1  markdown header (md-format_header_1)
  "\u{f026c}", // H2
  "\u{f026d}", // H3
  "\u{f026e}", // H4
  "\u{f026f}", // H5
  "\u{f0270}", // H6
] as const;

/** The `#{n} ` marker text for a level — mirrors the TUI's
 *  `header_prefix` (`"#".repeat(level) + " "`). */
export function headerPrefix(level: number): string {
  const n = Math.min(Math.max(level, 1), 6);
  return "#".repeat(n) + " ";
}

/** Per-level foreground colour as a CSS `var(--color-outl-header-fg-N)`
 *  injected name. The desktop palette installer writes
 *  `--color-outl-header-fg-1..6` from `Palette.header_fg_1..6`, exactly
 *  like the TUI's `theme.header_levels[level - 1]`. */
export function headerFgVar(level: number): string {
  const n = Math.min(Math.max(level, 1), 6);
  return `var(--color-outl-header-fg-${n})`;
}

import type { InlineToken } from "@outl/shared/api/types";

/** Strip the leading `#{n} ` marker from the first `Plain` token of a
 *  tokenized header body — desktop counterpart of
 *  `stripQuoteFromTokens`. A header's marker always lands in the first
 *  token and that token is always `Plain` (a `#` followed by a space
 *  is not a tag/ref, verified against `outl_md::inline::tokenize`), so
 *  stripping it is safe to call on every block with a `header_level`.
 *  A no-op when the first token isn't a matching `Plain`. */
export function stripHeaderMarkerFromTokens(
  tokens: InlineToken[],
  level: number,
): InlineToken[] {
  if (tokens.length === 0) return tokens;
  const prefix = headerPrefix(level);
  const first = tokens[0];
  if (first.kind === "plain" && first.value.startsWith(prefix)) {
    return [
      { ...first, value: first.value.slice(prefix.length) },
      ...tokens.slice(1),
    ];
  }
  return tokens;
}

/** Strip the marker from a header's raw text, for the empty-token
 *  fallback path. Returns the untouched input when the marker is
 *  absent. */
export function stripHeaderMarkerFromText(
  text: string,
  level: number,
): string {
  const prefix = headerPrefix(level);
  return text.startsWith(prefix) ? text.slice(prefix.length) : text;
}
