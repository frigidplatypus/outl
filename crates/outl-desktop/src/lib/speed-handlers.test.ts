import { beforeEach, describe, expect, it, vi } from "vitest";

const trust = vi.hoisted(() => ({ exportMarkdown: vi.fn() }));
vi.mock("@outl/shared/api/trust", () => trust);

import { quickCaptureOpen, closeQuickCapture } from "./quick-capture";
import { speedHandlers } from "./speed-handlers";

beforeEach(() => {
  trust.exportMarkdown.mockReset();
  closeQuickCapture();
});

describe("speedHandlers", () => {
  it("QuickCapture opens the capture box", () => {
    speedHandlers({ setError: vi.fn() }).QuickCapture?.();
    expect(quickCaptureOpen()).toBe(true);
  });

  it("ExportMarkdown says where the copy went", async () => {
    trust.exportMarkdown.mockResolvedValue({ dir: "/w/outl-export-1", pages: 1, assets: 0 });
    const setError = vi.fn();
    await speedHandlers({ setError }).ExportMarkdown?.();
    expect(trust.exportMarkdown).toHaveBeenCalledWith();
    expect(setError).toHaveBeenCalledWith("Exported 1 page as Markdown to /w/outl-export-1");
  });

  it("ExportMarkdown surfaces a failure instead of swallowing it", async () => {
    trust.exportMarkdown.mockRejectedValue(new Error("read-only volume"));
    const setError = vi.fn();
    await speedHandlers({ setError }).ExportMarkdown?.();
    expect(setError).toHaveBeenCalledWith("Export failed: read-only volume");
  });
});
