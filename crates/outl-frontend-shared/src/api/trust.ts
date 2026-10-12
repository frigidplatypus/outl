/**
 * Wire types and command wrappers for "trust you can see": the per-device
 * op-log summary and the plain Markdown export. Their own file because
 * `commands.ts` and `types.ts` are at their size ratchet; listed in the
 * wire-mirror files (`outl-tauri-shared/tests/ts_parser`) like the rest.
 */

import { invoke } from "@tauri-apps/api/core";

/**
 * One device's share of the op log, as returned by `device_activity`.
 * Mirrors the Rust `ActorActivityDto` in `outl-tauri-shared`.
 * `actor` is the op-log actor (ULID), not a peer node id.
 */
export interface ActorActivity {
  actor: string;
  ops: number;
  last_at_ms: number;
  is_local: boolean;
}

/**
 * What the op log holds from each device, most recently active first.
 * `complete` is `false` after a snapshot boot: the counts then cover only
 * the ops since that snapshot, and the panel has to say so.
 */
export interface DeviceActivity {
  actors: ActorActivity[];
  complete: boolean;
}

/** Where `export_markdown` wrote the copy. Mirrors `ExportReportDto`. */
export interface ExportReport {
  dir: string;
  pages: number;
  assets: number;
}

/** What the op log holds from each device. Read-only. */
export function deviceActivity(): Promise<DeviceActivity> {
  return invoke<DeviceActivity>("device_activity");
}

/**
 * Copy every page, journal and asset as plain Markdown into a new
 * `outl-export-<stamp>` folder under `destDir`, or next to the
 * workspace folder when omitted. Never overwrites: a second export gets
 * its own folder.
 */
export function exportMarkdown(destDir: string | null = null): Promise<ExportReport> {
  return invoke<ExportReport>("export_markdown", { destDir });
}
