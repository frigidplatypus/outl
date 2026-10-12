/**
 * The iOS capture inbox (RFC 0363): lines captured by the widget, the
 * "Add to today" App Intent and the share extension wait in the App
 * Group until the app drains them into the journal. Listed in the
 * wire-mirror files (`outl-tauri-shared/tests/ts_parser`).
 */

import { invoke } from "@tauri-apps/api/core";

/** What one drain did. Mirrors `CaptureDrainDto` in `outl-tauri-shared`. */
export interface CaptureDrain {
  landed: number;
  skipped: number;
  page_ids: string[];
}

/** Land every waiting capture. Zeros when there is no inbox. */
export function drainCaptureInbox(): Promise<CaptureDrain> {
  return invoke<CaptureDrain>("drain_capture_inbox");
}

/** The toast a drain earns, or `null` when nothing landed. */
export function drainMessage(d: CaptureDrain): string | null {
  if (d.landed === 0) return null;
  return d.landed === 1 ? "Added 1 capture to your journal." : `Added ${d.landed} captures to your journal.`;
}
