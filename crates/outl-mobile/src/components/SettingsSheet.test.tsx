/**
 * The header gear opened nothing at all.
 *
 * `SettingsSheet` drew its iOS toggle with a local helper component named
 * `Switch`, and `babel-preset-solid` auto-imports Solid's control-flow
 * `<Switch>` for that tag name regardless of what the module declares —
 * so `<Switch on={locked()} />` compiled to the built-in and the local
 * function was never called. A childless built-in `<Switch>` reads
 * `undefined.when` inside Solid's own memo and throws, which took down
 * the whole sheet render: tapping the gear flipped `settingsOpen` and
 * painted nothing.
 *
 * These pin that the sheet renders when `open` is set and that the toggle
 * in it is the one this file draws, so a helper named after a Solid
 * built-in fails here instead of on a device.
 */

import { render } from "solid-js/web";
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("../lib/haptics", () => ({ haptic: () => {} }));

import { SettingsSheet } from "./SettingsSheet";

let dispose: (() => void) | undefined;

function mount() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const onClose = vi.fn();
  const onOpenDevices = vi.fn();
  dispose = render(
    () => (
      <SettingsSheet open onClose={onClose} onOpenDevices={onOpenDevices} />
    ),
    host,
  );
  return { onClose, onOpenDevices };
}

/** The row that reaches pairing, found by its label rather than by
 *  position: it has to stay findable if the sheet grows. */
function devicesRow(): HTMLButtonElement | undefined {
  return [...document.body.querySelectorAll("button")].find((b) =>
    b.textContent?.includes("Devices"),
  ) as HTMLButtonElement | undefined;
}

/** The sheet renders into a portal-less fixed overlay on `document.body`. */
function lockRow(): HTMLButtonElement | null {
  return document.body.querySelector<HTMLButtonElement>('[role="switch"]');
}

afterEach(() => {
  // happy-dom here exposes no `localStorage`; the toolbar store is
  // defensive about that, so the sheet renders either way.
  globalThis.localStorage?.clear();
  dispose?.();
  dispose = undefined;
  document.body.innerHTML = "";
});

describe("SettingsSheet", () => {
  it("renders the toolbar lock row when open", () => {
    mount();
    const row = lockRow();
    expect(row).not.toBeNull();
    expect(row!.textContent).toContain("Lock button order");
    expect(row!.getAttribute("aria-checked")).toBe("false");
  });

  it("draws its own toggle, not a Solid control-flow component", () => {
    mount();
    // The knob only exists if the local helper ran: a Solid built-in of
    // the same tag name renders its children instead (and throws when
    // there are none).
    const knob = lockRow()!.querySelector<HTMLElement>(
      '[aria-hidden="true"] > span',
    );
    expect(knob).not.toBeNull();
    expect(knob!.style.left).toBe("2px");
  });

  it("moves the knob and flips aria-checked on tap", () => {
    mount();
    lockRow()!.click();
    expect(lockRow()!.getAttribute("aria-checked")).toBe("true");
    const knob = lockRow()!.querySelector<HTMLElement>(
      '[aria-hidden="true"] > span',
    );
    expect(knob!.style.left).toBe("22px");
  });

  // P2P pairing was reachable only by pressing and holding the sync dot
  // (`64fe9c53`), a gesture nothing announces, and Settings never named
  // it — so on the phone the feature read as gone. The desktop has had
  // Settings → Sync the whole time.
  it("offers device sync, the way the desktop's Settings does", () => {
    const { onOpenDevices } = mount();
    const row = devicesRow();
    expect(row).toBeDefined();
    row!.click();
    expect(onOpenDevices).toHaveBeenCalledTimes(1);
  });

  // `DevicesSheet` is `z-50` and this sheet is `z-[55]`, so leaving this
  // one open would paint the devices sheet underneath it.
  it("closes itself when it hands over to the devices sheet", () => {
    const { onClose } = mount();
    devicesRow()!.click();
    expect(onClose).toHaveBeenCalled();
  });
});
