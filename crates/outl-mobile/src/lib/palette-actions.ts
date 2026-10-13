/**
 * Actions the phone's switcher lists, the mobile half of the command
 * palette (`Action::OpenCommandPalette`). Same rules as the desktop's
 * `palette-results.ts`: actions show under pages once a query matches
 * their label, and a leading `>` narrows the list to actions only.
 *
 * The phone has no chords, so a row is a label and what it opens.
 * Labels match the shortcut catalog's descriptions where one exists, so
 * the same words find the same thing on both clients.
 */
export type MobileAction =
  | "OpenToday"
  | "QuickCapture"
  | "TaskList"
  | "OpenReminders"
  | "OpenCalendar"
  | "SyncNow"
  | "OpenDevices"
  | "OpenPlugins"
  | "OpenSettings"
  | "FoldAll"
  | "UnfoldAll";

export interface MobileActionRow {
  action: MobileAction;
  label: string;
  /** Extra words that should find it ("sync" finds Devices). */
  keywords?: string;
}

export const COMMAND_PREFIX = ">";
/** Actions shown under pages for a plain query. */
export const ACTION_LIMIT = 5;

export const MOBILE_ACTIONS: readonly MobileActionRow[] = [
  { action: "OpenToday", label: "Open today's journal", keywords: "journal day" },
  { action: "QuickCapture", label: "Quick capture to today", keywords: "add note" },
  { action: "TaskList", label: "Open task list", keywords: "todo doing" },
  { action: "OpenReminders", label: "Open reminders", keywords: "remind notifications" },
  { action: "OpenCalendar", label: "Open calendar", keywords: "date month" },
  { action: "SyncNow", label: "Sync now", keywords: "refresh pull" },
  { action: "OpenDevices", label: "Devices and sync", keywords: "pending merged peers trust history" },
  { action: "OpenPlugins", label: "Plugins", keywords: "marketplace extensions" },
  { action: "OpenSettings", label: "Open settings", keywords: "preferences theme focus export" },
  { action: "FoldAll", label: "Fold all blocks", keywords: "collapse" },
  { action: "UnfoldAll", label: "Unfold all blocks", keywords: "expand" },
];

export function isCommandQuery(query: string): boolean {
  return query.trim().startsWith(COMMAND_PREFIX);
}

/**
 * Rows for a query. `>` lists every match (all actions for a bare `>`);
 * a plain query lists up to {@link ACTION_LIMIT}, and none when empty so
 * the switcher still opens on pages.
 */
export function matchActions(query: string): MobileActionRow[] {
  const raw = query.trim();
  const command = raw.startsWith(COMMAND_PREFIX);
  const q = (command ? raw.slice(COMMAND_PREFIX.length) : raw).trim().toLowerCase();
  if (!command && !q) return [];
  const hits = MOBILE_ACTIONS.filter((a) =>
    `${a.label} ${a.keywords ?? ""}`.toLowerCase().includes(q),
  );
  return command ? hits : hits.slice(0, ACTION_LIMIT);
}
