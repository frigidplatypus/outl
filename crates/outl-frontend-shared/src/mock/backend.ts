import type { Backlink, Palette, PageMeta } from "../api/types";
import presets from "./presets.json";
import { journalSlug, MockWorkspace, shiftDay } from "./workspace";

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

const palettes = presets as Palette[];

/**
 * A handful of the catalog's chrome bindings, so the palette and the
 * keyboard have something to work with. The real catalog is
 * `outl_shortcuts::default_bindings`; this is not a copy of it.
 */
const META = 0b1000;
const chord = (value: string) => [{ mods: META, key: { kind: "Char", value } }];
const BINDINGS = [
  { chord: chord("p"), mode: "global", action: { kind: "OpenPicker" }, description: "Open quick switcher" },
  { chord: chord("k"), mode: "normal", action: { kind: "OpenCommandPalette" }, description: "Command palette" },
  { chord: chord("j"), mode: "global", action: { kind: "OpenToday" }, description: "Open today's journal" },
  { chord: chord("\\"), mode: "global", action: { kind: "ToggleSidebar" }, description: "Toggle sidebar" },
  { chord: chord(","), mode: "global", action: { kind: "OpenSettings" }, description: "Open settings" },
];
const palette = (name: unknown) =>
  palettes.find((p) => p.name === name) ?? palettes[0];

/**
 * Answers the Tauri commands the clients call, from a {@link MockWorkspace}.
 * Anything missing here resolves to `null` and logs once, so a new
 * command shows up in the console instead of breaking the page.
 */
export function createMockBackend(ws = new MockWorkspace(), today = new Date()) {
  const todaySlug = journalSlug(today);
  const settings = {
    last_workspace: "/mock/workspace",
    vim_mode: false,
    theme: "outl-light",
    theme_dark: "outl",
    theme_mode: "auto",
    font_size: 15,
    sync_transport: "iroh",
    backlinks_order: "newest",
    reminders_enabled: false,
    reminders_quiet_hours: "",
  };

  const page = (args: Args) => {
    const p = ws.byId(String(args.pageId));
    if (!p) throw new Error(`mock: no page ${String(args.pageId)}`);
    return p;
  };
  const open = (slug: string) => {
    const p = ws.bySlug(slug) ?? (/^\d{4}-\d{2}-\d{2}$/.test(slug) ? ws.ensureJournal(slug) : undefined);
    if (!p) throw new Error(`mock: no page ${slug}`);
    return ws.view(p);
  };
  const metas = (): PageMeta[] => [...ws.pages.values()].map((p) => p.meta);
  const search = (query: unknown) => {
    const q = String(query ?? "").toLowerCase();
    return metas().filter((m) => m.title.toLowerCase().includes(q) || m.slug.includes(q));
  };

  const backlinks = (slug: string): Backlink[] => {
    const target = ws.bySlug(slug);
    if (!target) return [];
    const names = [target.meta.slug, target.meta.title.toLowerCase()];
    const out: Backlink[] = [];
    for (const [src, node, path] of ws.walk()) {
      if (src === target) continue;
      const hit = node.tokens.some((t) => (t.kind === "ref" || t.kind === "tag") && names.includes(t.value.toLowerCase()));
      if (!hit) continue;
      out.push({
        block_id: node.id,
        todo: node.todo,
        source_page: src.meta,
        ancestors: path.map((a) => ({ id: a.id, text: a.text })),
        source_block: node,
        source_block_path: [],
      });
    }
    return out;
  };

  const handlers: Record<string, Handler> = {
    current_workspace: () => settings.last_workspace,
    workspace_stats: () => ({ blocks: [...ws.walk()].length, ops: 0, actor: "mock", storage_root: settings.last_workspace, ready: true }),
    get_settings: () => settings,
    update_settings: (a) => Object.assign(settings, a.next),
    get_theme_config: () => ({ preset: settings.theme, preset_dark: settings.theme_dark, mode: settings.theme_mode }),
    get_theme: (a) => palette(a.name ?? settings.theme),
    list_themes: () => palettes.map((p) => p.name),
    list_shortcut_bindings: () => BINDINGS,
    list_action_support: () => [],

    today_slug_cmd: () => todaySlug,
    previous_day: (a) => shiftDay(String(a.slug), -1),
    next_day: (a) => shiftDay(String(a.slug), 1),
    date_title: (a) => {
      const [y, m, d] = String(a.slug).split("-").map(Number);
      return new Date(y, m - 1, d).toLocaleDateString("en-US", { weekday: "long", month: "long", day: "numeric", year: "numeric" });
    },
    open_today_journal: () => open(todaySlug),
    open_journal_for: (a) => open(String(a.slug)),
    open_page_by_slug: (a) => open(String(a.slug)),
    open_ref: (a) => open(String(a.target)),
    resolve_ref: (a) => ws.bySlug(String(a.target))?.meta ?? null,
    list_all_pages: metas,
    search_pages: (a) => search(a.query),
    search_persons: (a) => search(a.query).filter((m) => m.slug === "ana"),
    search_blocks: (a) => {
      const q = String(a.query ?? "").toLowerCase();
      return [...ws.walk()]
        .filter(([, n]) => n.text.toLowerCase().includes(q))
        .slice(0, 25)
        .map(([p, n]) => ({ handle: n.id, text: n.text, source_slug: p.meta.slug }));
    },
    page_backlinks: (a) => ({
      backlinks: backlinks(String(a.slug)),
      backlinks_order: settings.backlinks_order,
      namespace_backlinks: [],
      namespace_backlinks_total: 0,
      namespace_children: [],
    }),
    resolve_page_labels: (a) => (a.nodeIds as string[]).map(() => ""),
    resolve_embeds: () => [],
    known_property_keys: () => [],
    list_templates_cmd: () => [],
    page_timeline: () => [],
    outl_emoji_search: () => [],
    outl_peer_list: () => [],
    outl_peer_status: () => null,
    list_reminders: () => [],
    plugin_list: () => [],
    plugin_toolbar: () => [],
    plugin_keybindings: () => [],
    plugin_transformers: () => [],
    reminder_action_catalog: () => [],
    reload_workspace: () => null,
    outl_sync_now: () => null,
    take_pending_deep_link: () => null,
    take_pending_open_file: () => null,

    edit_block: (a) => {
      const p = page(a);
      const at = ws.locate(p, String(a.id));
      if (at) ws.setText(at[0][at[1]], String(a.text));
      return ws.view(p);
    },
    create_block: (a) => {
      const p = page(a);
      const node = ws.insertAfter(p, (a.afterId as string | null) ?? null, (a.text as string | null) ?? "");
      return { view: ws.view(p), new_id: node.id };
    },
    split_block: (a) => {
      const p = page(a);
      const node = ws.insertAfter(p, String(a.id));
      return { view: ws.view(p), new_id: node.id };
    },
    toggle_todo: (a) => {
      const p = page(a);
      const at = ws.locate(p, String(a.id));
      if (at) {
        const n = at[0][at[1]];
        const body = n.text.replace(/^(TODO|DOING|DONE) /, "");
        ws.setText(n, n.todo === null ? `TODO ${body}` : n.todo === "DONE" ? body : `DONE ${body}`);
      }
      return ws.view(p);
    },
    delete_block: (a) => {
      const p = page(a);
      const at = ws.locate(p, String(a.id));
      if (at) at[0].splice(at[1], 1);
      return ws.view(p);
    },
    set_block_collapsed: (a) => {
      const p = page(a);
      const at = ws.locate(p, String(a.id));
      if (at) at[0][at[1]].collapsed = Boolean(a.collapsed);
      return ws.view(p);
    },
  };

  const missed = new Set<string>();
  return (cmd: string, args: Args = {}): unknown => {
    const h = handlers[cmd];
    if (h) return h(args);
    if (/^plugin:[a-z-]+\|/.test(cmd)) return null;
    if (!missed.has(cmd)) {
      missed.add(cmd);
      console.info(`[outl mock] no handler for "${cmd}", answering null`);
    }
    return null;
  };
}
