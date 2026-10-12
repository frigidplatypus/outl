/**
 * The 30-second tour: three tips (blocks, links, daily) shown in the
 * empty state until the user dismisses them. Same words on both
 * clients; only the gesture hint differs.
 */

export const TIPS_KEY = "outl.first-run-tips";

export type TipClient = "desktop" | "mobile";

export interface Tip {
  id: "blocks" | "links" | "daily";
  glyph: string;
  title: string;
  body: string;
}

export function firstRunTips(client: TipClient): Tip[] {
  const indent = client === "desktop" ? "Tab and Shift+Tab" : "the indent keys above the keyboard";
  const today = client === "desktop" ? "⌘J" : "the Today button";
  return [
    {
      id: "blocks",
      glyph: "•",
      title: "Every line is a block",
      body: `Press Enter for a new one, ${indent} to nest it. Fold a block to hide its children.`,
    },
    {
      id: "links",
      glyph: "[[",
      title: "Link pages as you type",
      body: "Type [[ to link a page or create it. Every link shows up as a backlink on the other side.",
    },
    {
      id: "daily",
      glyph: "◷",
      title: "Today has a page",
      body: `Your journal opens on today. Jump back to it any time with ${today}.`,
    },
  ];
}

/**
 * A page that exists but holds nothing yet: at least one block, every
 * one empty and childless. A fresh journal day lands here (it opens on
 * one empty block), which is where a new user first looks.
 */
export function isBlankOutline(blocks: readonly { text: string; children: readonly unknown[] }[]): boolean {
  return blocks.length > 0 && blocks.every((b) => b.text.trim() === "" && b.children.length === 0);
}

type Store = Pick<Storage, "getItem" | "setItem">;

function safeStore(): Store | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export function tipsDismissed(store: Store | null = safeStore()): boolean {
  return store?.getItem(TIPS_KEY) === "done";
}

export function dismissTips(store: Store | null = safeStore()): void {
  try {
    store?.setItem(TIPS_KEY, "done");
  } catch {
    // Not persisting just means the tips show once more.
  }
}
