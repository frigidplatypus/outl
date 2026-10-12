/**
 * Coming back to the journal: pull peers' changes and drain the iOS
 * capture inbox (RFC 0363). Drains run on mount, on every return to the
 * foreground, and when the native side calls
 * `window.__outlCapturesArrived()` after an extension wrote a capture
 * while the app was open.
 *
 * Must be called from inside the component body (`onCleanup` needs an
 * owner).
 */

import { onCleanup } from "solid-js";

import { drainCaptureInbox, drainMessage } from "@outl/shared/api/capture";

type WindowWithInbox = Window & { __outlCapturesArrived?: () => void };

export interface InboxDeps {
  /** Pull peers' ops and reload the view (`pullAndReload`). */
  pull: () => unknown;
  /** Say what landed. */
  notify: (text: string) => void;
}

/** One drain; reloads the view and says so when anything landed. */
export async function drainOnce(deps: InboxDeps): Promise<number> {
  try {
    const d = await drainCaptureInbox();
    const msg = drainMessage(d);
    if (msg) {
      await deps.pull();
      deps.notify(msg);
    }
    return d.landed;
  } catch {
    // best-effort: a capture that failed to land stays in the inbox
    return 0;
  }
}

export function installForegroundRefresh(deps: InboxDeps) {
  const onVisible = () => {
    if (document.visibilityState !== "visible") return;
    void deps.pull();
    void drainOnce(deps);
  };
  document.addEventListener("visibilitychange", onVisible);
  const win = window as WindowWithInbox;
  const arrived = () => void drainOnce(deps);
  win.__outlCapturesArrived = arrived;
  onCleanup(() => {
    document.removeEventListener("visibilitychange", onVisible);
    if (win.__outlCapturesArrived === arrived) delete win.__outlCapturesArrived;
  });
  void drainOnce(deps);
}
