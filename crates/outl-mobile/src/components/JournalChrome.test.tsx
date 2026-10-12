/**
 * The sync dot replaced the separate refresh glyph in the top capsule
 * (#362): tap syncs now, press-and-hold opens devices. These pin that
 * split, that a tap during a pass is dropped, and that the dot spins for
 * both the manual and the automatic pass.
 */

import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/haptics", () => ({ haptic: () => {} }));

import { JournalChrome } from "./JournalChrome";

let dispose: (() => void) | undefined;

const noop = () => {};
const lp = {
  onPointerDown: noop,
  onPointerMove: noop,
  onPointerUp: noop,
  cancel: noop,
  consumedClick: () => false,
};

function mount(opts: { syncing?: boolean; refreshing?: boolean } = {}) {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const onRefresh = vi.fn();
  const onOpenDevices = vi.fn();
  const [syncing, setSyncing] = createSignal(opts.syncing ?? false);
  const [refreshing, setRefreshing] = createSignal(opts.refreshing ?? false);
  dispose = render(
    () => (
      <JournalChrome
        view={null}
        online
        peersUp
        syncing={syncing()}
        refreshing={refreshing()}
        todaySlug={null}
        toolbarButtons={[]}
        titleLongPress={lp}
        onJumpToday={noop}
        onPrevDay={noop}
        onNextDay={noop}
        onFoldAll={noop}
        onUnfoldAll={noop}
        onRefresh={onRefresh}
        onRunToolbarButton={noop}
        onOpenReminders={noop}
        onOpenPlugins={noop}
        onOpenDevices={onOpenDevices}
        onOpenSettings={noop}
      />
    ),
    host,
  );
  const dot = host.querySelector<HTMLButtonElement>('button[title="Sync now"]')!;
  return { host, dot, onRefresh, onOpenDevices, setSyncing, setRefreshing };
}

function pointer(type: string): Event {
  const e = new Event(type, { bubbles: true }) as Event & {
    clientX: number;
    clientY: number;
  };
  Object.assign(e, { clientX: 0, clientY: 0 });
  return e;
}

beforeEach(() => vi.useFakeTimers());

afterEach(() => {
  vi.useRealTimers();
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

describe("JournalChrome sync dot", () => {
  it("has no separate refresh button any more", () => {
    const { host } = mount();
    const syncButtons = [...host.querySelectorAll("button")].filter((b) =>
      /sync now/i.test(b.getAttribute("aria-label") ?? ""),
    );
    expect(syncButtons).toHaveLength(1);
  });

  it("syncs now on tap", () => {
    const { dot, onRefresh, onOpenDevices } = mount();
    expect(dot.getAttribute("aria-label")).toMatch(/^Sync now/);
    dot.click();
    expect(onRefresh).toHaveBeenCalledTimes(1);
    expect(onOpenDevices).not.toHaveBeenCalled();
  });

  it("opens devices on press-and-hold and swallows the click after", () => {
    const { dot, onRefresh, onOpenDevices } = mount();
    dot.dispatchEvent(pointer("pointerdown"));
    vi.advanceTimersByTime(500);
    dot.dispatchEvent(pointer("pointerup"));
    dot.click();
    expect(onOpenDevices).toHaveBeenCalledTimes(1);
    expect(onRefresh).not.toHaveBeenCalled();
  });

  it("ignores taps and spins while a pass runs", () => {
    const { dot, onRefresh, setSyncing, setRefreshing } = mount();
    for (const set of [setSyncing, setRefreshing]) {
      set(true);
      expect(dot.getAttribute("aria-busy")).toBe("true");
      expect(dot.querySelector(".animate-spin")).not.toBeNull();
      dot.click();
      expect(onRefresh).not.toHaveBeenCalled();
      set(false);
    }
    expect(dot.querySelector(".animate-spin")).toBeNull();
    dot.click();
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });
});
