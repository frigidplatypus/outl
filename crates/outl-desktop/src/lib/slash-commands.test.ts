import { describe, expect, it } from "vitest";

import { rankSlashCommands, templateSlashCommands } from "./slash-commands";
import type { PluginCommand, TemplateDto } from "@outl/shared/api/types";

const cmd = (command_id: string, title: string): PluginCommand => ({
  plugin_id: `app.outl.examples.${command_id}`,
  command_id,
  title,
});

const ALL: PluginCommand[] = [
  cmd("stats", "Workspace statistics"),
  cmd("greet", "Greet me"),
  cmd("pick", "Pick a random task"),
  cmd("todo-archive-done", "Archive DONE blocks"),
];

describe("rankSlashCommands", () => {
  it("ranks the id-prefix match on top — the `/sta` report", () => {
    const out = rankSlashCommands(ALL, "sta");
    expect(out[0].command_id).toBe("stats");
  });

  it("matches by full id and keeps it first", () => {
    const out = rankSlashCommands(ALL, "stats");
    expect(out[0].command_id).toBe("stats");
  });

  it("returns every command for an empty query (bare `/`)", () => {
    expect(rankSlashCommands(ALL, "")).toHaveLength(ALL.length);
  });

  it("falls back to a title match when the id does not match", () => {
    // No id contains "archive"; only the title "Archive DONE blocks".
    const out = rankSlashCommands(ALL, "archive");
    expect(out.map((c) => c.command_id)).toContain("todo-archive-done");
  });

  it("drops commands that match neither id nor title", () => {
    expect(rankSlashCommands(ALL, "zzzz")).toEqual([]);
  });

  it("ranks id-substring above title-substring", () => {
    // Query "e": id "greet" (substring) must outrank a title-only hit.
    const out = rankSlashCommands(ALL, "gree");
    expect(out[0].command_id).toBe("greet");
  });
});

const tpl = (name: string, extra: Partial<TemplateDto> = {}): TemplateDto => ({
  name,
  slug: `template-${name}`,
  ...extra,
});

describe("templateSlashCommands anchor surfacing", () => {
  it("shows the anchor when it is not the default", () => {
    const [row] = templateSlashCommands([tpl("meeting", { insert: "after" })]);
    expect(row.title).toContain("→ insert after");
  });

  it("warns on an unrecognized anchor the backend is ignoring", () => {
    const [row] = templateSlashCommands([
      tpl("meeting", { insert: "sideways", insert_unrecognized: true }),
    ]);
    expect(row.title).toContain("⚠ insert:: sideways not recognized");
  });

  it("stays silent for the `child` default", () => {
    // `child` / absent nest silently — annotating the default is noise.
    expect(templateSlashCommands([tpl("meeting")])[0].title).not.toContain(
      "insert",
    );
  });
});

describe("templateSlashCommands kind marker", () => {
  it("marks a structural template as clone", () => {
    expect(templateSlashCommands([tpl("meeting")])[0].title).toContain("· clone");
  });

  it("marks a callable template as call", () => {
    expect(templateSlashCommands([tpl("report", { callable: true })])[0].title).toContain(
      "· call",
    );
  });

  it("suppresses the anchor for a callable template", () => {
    // `insert::` is ignored for a callable template (it runs its code block),
    // so surfacing it here would mislead about what picking does.
    const title = templateSlashCommands([
      tpl("report", { callable: true, insert: "after" }),
    ])[0].title;
    expect(title).not.toContain("insert");
  });
});

