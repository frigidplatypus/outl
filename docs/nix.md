# Nix

outl is a [flake](../flake.nix).
One input exposes two surfaces: a set of **packages** and a **home-manager module** for per-user configuration and background sync.

| Flake output | Source | What it is |
|---|---|---|
| `packages.outl` *(default)* | [`flake.nix`](../flake.nix) | The CLI (`outl`) and the TUI (`outl-tui`). |
| `packages.outl-desktop` | [`flake.nix`](../flake.nix) | The Tauri 2 desktop app. |
| `homeManagerModules.default` | [`hm-module.nix`](../hm-module.nix) | `programs.outl` — manages `~/.config/outl/config.toml`, optionally installs the packages and a per-user sync service. |

> **Linux only.**
> Packages build for `x86_64-linux` and `aarch64-linux`.
> There is no Nix-built macOS package: the Tauri desktop build needs Apple SDK frameworks that current nixpkgs no longer exposes as stable attributes.
> On macOS the home-manager module still writes your `config.toml`; you supply the binary yourself (see the [Homebrew tap](homebrew.md)).

## The flake

The flake pins `nixpkgs-unstable`, `flake-utils`, and [`rust-overlay`](https://github.com/oxalica/rust-overlay), which reads the compiler toolchain straight from [`rust-toolchain.toml`](../rust-toolchain.toml).

It also declares a Cachix substituter (`https://outl.cachix.org`, see the `nixConfig` block in [`flake.nix`](../flake.nix)), so installing outl pulls a pre-built binary instead of compiling Rust.
You do not need a Rust toolchain to use either surface.

## Packages

### CLI + TUI — `packages.outl`

One derivation builds both binaries (`cargo build -p outl-cli -p outl-tui`):

- `outl` — the CLI (workspace ops, `serve`, `peer`, `doctor`, …).
- `outl-tui` — the terminal UI.

```bash
nix profile add github:outlmd/outl     # default package = outl
outl --help
outl-tui
```

Or run without installing:

```bash
nix run github:outlmd/outl -- --help
```

### Desktop — `packages.outl-desktop`

The Tauri 2 app.
The build compiles the Solid frontend with Bun first, then builds the Rust shell with the `production` feature so the webview embeds the frontend instead of pointing at a dev server.
The package also emits a freedesktop `.desktop` entry and hicolor icons into its `$out/share`, so a full desktop session picks up the launcher and icon with no extra setup (home-manager extends this to `~/.local/share` for bare Wayland launchers — see the module below).

```bash
nix profile add github:outlmd/outl#outl-desktop
outl-desktop
```

### Dev shell

`devShells.default` gives you the pinned Rust toolchain plus `cargo-tauri`, `bun`, and `nodejs`, with the package dependencies already on the path:

```bash
nix develop github:outlmd/outl
```

## Home-manager module

[`hm-module.nix`](../hm-module.nix) is exposed as `homeManagerModules.default` under the `programs.outl` namespace.
Its job is **configuration management**: it renders `~/.config/outl/config.toml` from typed options, and (on Linux) can install the packages and start a per-user background sync.

The whole `programs.outl` surface is a single module block.
Write your configuration as **one `settings = { … }` attribute set** — every key is a plain option under it — rather than scattering dotted `settings.theme.preset = …` lines across the file:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    outl.url = "github:outlmd/outl";
  };

  outputs = { nixpkgs, outl, ... }: {
    homeConfigurations.you = nixpkgs.lib.homeManagerConfiguration {
      pkgs = nixpkgs.legacyPackages.x86_64-linux;
      modules = [
        outl.homeManagerModules.default

        ({ pkgs, ... }: {
          programs.outl = {
            enable = true;
            installDesktop = true;        # also put outl-desktop on PATH + a launcher

            # One attribute set. `managed = true` is the default and is what
            # keeps config.toml a symlink home-manager can re-own, so pin
            # `workspace.last` here — the app can no longer store it itself.
            settings = {
              workspace.last = "/home/you/outl";   # absolute path
              theme.preset = "gruvbox";            # themes the terminal AND desktop
              editor.vimMode = true;
              sync.transport = "iroh";
              reminders.quietHours = "22:00-07:00";
            };

            # Optional per-user background sync (Linux only). The path is
            # passed straight to `outl serve` in a systemd ExecStart, which
            # does NOT expand `~` — give it an absolute path.
            services.sync = {
              enable = true;
              workspace = "/home/you/notes";
            };
          };
        })
      ];
    };
  };
}
```

When enabled, the module:

- Writes `~/.config/outl/config.toml` from `programs.outl.settings` (every key is typed; anything not yet modeled goes in `settings.extraConfig`).
- On Linux, adds `programs.outl.package` to `home.packages` (and `outl-desktop` too, if `installDesktop`).
- If `installDesktop` (Linux), also installs the freedesktop launcher entry and the hicolor icons into `~/.local/share` (`~/.local/share/applications` and `~/.local/share/icons`), symlinked from the `outl-desktop` package so the app menu, the "Open With" menu and Wayland launchers (e.g. `rofi -show drun` under a tiling WM) show the right entry and icon.
  The package is the single source — these are symlinks, not a second copy.
- If `services.sync.enable`, starts a **user** systemd unit `outl-sync` that runs `outl serve --workspace <path>` and restarts on failure.
  `services.sync.watch` and `.sync` toggle the watcher and endpoint halves (`--no-watch` / `--no-sync`); `.rustLog` sets `RUST_LOG`.
  The service runs as *you*, using your own identity (`~/.outl`) and device store (`~/.config/outl`) — pairing is plain `outl peer pair`, and the daemon picks up new peers on its own.

> The service runs while you are logged in.
> For a dedicated always-on box (a NAS, a VPS) where nobody sits at a login, enable lingering so your user manager stays alive without a session: `loginctl enable-linger <user>`.
> That is what keeps the box paired 24/7.

### Settings

`programs.outl.settings` mirrors [`outl.toml`](config.md).
Write it as **one attribute set**; each entry below is a key inside it.
The generated `config.toml` keys are `snake_case`; the Nix option names are `camelCase` — the module maps one to the other.

| Option (under `settings`) | Type | Default | Generated key · meaning |
|---|---|---|---|
| `managed` | bool | `true` | `managed` · top-level directive so no client rewrites `config.toml`. See [Declarative config](#declarative-config). |
| `workspace.last` | str \| null | `null` | `[workspace] last` · absolute path the desktop reopens on launch. Pin it: `managed` freezes the app's own write. |
| `theme.preset` | enum | `"outl-light"` | `[theme] preset` · light side of the pair. One of `outl`, `outl-light`, `default-dark`, `light`, `logseq-light`, `dracula`, `solarized-dark`, `nord`, `monokai`, `gruvbox`. |
| `theme.presetDark` | enum \| null | `null` | `[theme] preset_dark` · dark side the TUI renders under `mode = "auto"`/`"dark"`. `null` follows `preset`. |
| `theme.mode` | enum | `"auto"` | `[theme] mode` · `"light"`, `"dark"` or `"auto"`. A terminal reads `auto` as **dark**. |
| `editor.vimMode` | bool | `true` | `[editor] vim_mode` · vim-style modal bindings (desktop). |
| `editor.fontSize` | int | `15` | `[editor] font_size` · outline font size in px (desktop only). |
| `calendar.timezone` | str \| null | `null` | `[calendar] timezone` · IANA name, e.g. `"Europe/London"`. OS timezone when unset. |
| `sync.transport` | enum | `"iroh"` | `[sync] transport` · `"iroh"` (P2P QUIC) or `"file"` (iCloud / shared FS). |
| `sync.relayUrl` | str \| null | `null` | `[sync] relay_url` · custom iroh relay. outl's default when unset. |
| `display.backlinksOrder` | enum | `"newest"` | `[display] backlinks_order` · `"newest"` or `"oldest"`. |
| `assets.maxBytes` | int | `104857600` | `[assets] max_bytes` · cap on one uploaded file. `0` = unbounded. |
| `reminders.enabled` | bool | `true` | `[reminders] enabled` · whether this device delivers reminder notifications. |
| `reminders.quietHours` | str \| null | `null` | `[reminders] quiet_hours` · defer window, e.g. `"22:00-07:00"`. |
| `snapshot.enabled` | bool | `true` | `[snapshot] enabled` · materialized-state snapshots for faster boot. |
| `snapshot.opThreshold` | int | `10000` | `[snapshot] op_threshold` · ops between snapshot writes. |
| `storage.lruCap` | int | `20000` | `[storage] lru_cap` · max ops held in memory. `0` = unbounded. |
| `tui.mouseCapture` | bool | `false` | `[tui] mouse_capture` · capture mouse in the TUI (disables terminal text selection). |
| `backup.enabled` | bool | `true` | `[backup] enabled` · automatic git snapshots of the workspace. |
| `backup.intervalMinutes` | int | `30` | `[backup] interval_minutes` · minimum minutes between automatic snapshots. |
| `extraConfig` | attrs | `{}` | Deep-merged **last**, for keys the module has not modelled yet. Write its keys in `snake_case`, matching `config.toml`. |
| `extraConfig.tui.icons` | str | `"emoji"` | TUI icon style: `"emoji"` or `"nerd-font"`. |

Every option carries a `description` home-manager renders as documentation, so `nix-option-lookup`/the generated manpage is authoritative.
The source is [`hm-module.nix`](../hm-module.nix); the meaning of each key is in [Configuration](config.md).

### Declarative config

`programs.outl` writes `config.toml` as a **symlink** into the Nix store.
That only survives if nothing replaces the file — and outl's clients write their settings by **atomic rename**, which detaches the symlink and leaves a plain regular file.
The next `home-manager switch` then refuses to overwrite its own managed target.

So the module sets `managed = true` in the generated file by default, and outl treats that as "do not rewrite":

- Every client (`outl`, `outl-desktop`, `outl-tui`, mobile) skips its config write, so the symlink stays a symlink across switches.
- The **desktop Settings modal disables Save** and points you at `programs.outl.settings`; the theme picker still previews live.
- Because the modal can no longer persist it, **`workspace.last` freezes** unless you pin it. Pin it declaratively (inside the same `settings` set) to keep the desktop reopening your workspace:

  ```nix
  programs.outl.settings = {
    workspace.last = "/home/you/outl";   # absolute path
  };
  ```

  (Omit it and the desktop just opens the workspace picker on launch — every reader falls through cleanly.)

To go back to app-managed settings (the client persists its own state again), set `managed = false` in the same set.
A client write will then detach the symlink again, so also set `xdg.configFile."outl/config.toml".force = true` to let home-manager reclaim it on the next switch:

  ```nix
  programs.outl.settings = {
    managed = false;
  };
  ```

> **Migrating a machine that already hit the conflict.**
> If you were using this module before the `managed` default, your `~/.config/outl/config.toml` is already a stray regular file and the switch is failing with *"file exists and cannot be overridden"*.
> The module already sets `force = true` on that target, so a `home-manager switch` now takes it back over; from then on the `managed` directive keeps it a symlink. (Drop the stray file by hand first only if you'd edited it.)

## Which do I use?

| Situation | Use |
|---|---|
| Just want the CLI/TUI on any Nix machine | `packages.outl` |
| Want `config.toml` managed + optional logged-in sync on a laptop/desktop | `homeManagerModules.default` |
| A dedicated box that must stay paired 24/7 (NAS, VPS) | `homeManagerModules.default` + `loginctl enable-linger <user>` |
