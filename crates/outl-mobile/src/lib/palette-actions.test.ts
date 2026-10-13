import { describe, expect, it } from "vitest";
import { ACTION_LIMIT, MOBILE_ACTIONS, isCommandQuery, matchActions } from "./palette-actions";

describe("mobile palette actions", () => {
  it("lists nothing for an empty query, so the switcher opens on pages", () => {
    expect(matchActions("")).toEqual([]);
  });

  it("a bare > lists every action", () => {
    expect(matchActions(">")).toHaveLength(MOBILE_ACTIONS.length);
    expect(isCommandQuery("  >set")).toBe(true);
  });

  it("matches labels and keywords, case insensitive", () => {
    expect(matchActions("REMIND").map((a) => a.action)).toEqual(["OpenReminders"]);
    expect(matchActions("> pending").map((a) => a.action)).toEqual(["OpenDevices"]);
  });

  it("caps a plain query and leaves > uncapped", () => {
    expect(matchActions("o").length).toBeLessThanOrEqual(ACTION_LIMIT);
    expect(matchActions(">o").length).toBeGreaterThan(ACTION_LIMIT);
  });
});
