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

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    outl.url = "github:outlmd/outl";
  };

  outputs = { nixpkgs, outl, ... }: {
    homeConfigurations.you = nixpkgs.legacyPackages.x86_64-linux.home.manager.config {
      imports = [ outl.homeManagerModules.default ];

      programs.outl = {
        enable = true;
        installDesktop = true;            # also put outl-desktop on the PATH

        settings = {
          theme.preset = "gruvbox";
          editor.vimMode = true;
          sync.transport = "iroh";
          reminders.quietHours = "22:00-07:00";
        };

        # Optional per-user background sync (Linux only):
        services.sync = {
          enable = true;
          workspace = "/home/you/notes";
        };
      };
    };
  };
}
```

When enabled, the module:

- Writes `~/.config/outl/config.toml` from `programs.outl.settings` (every key is typed; anything not yet modeled goes in `settings.extraConfig`).
- On Linux, adds `programs.outl.package` to `home.packages` (and `outl-desktop` too, if `installDesktop`).
- If `services.sync.enable`, starts a **user** systemd unit `outl-sync` that runs `outl serve --workspace <path>` and restarts on failure.
  `services.sync.watch` and `.sync` toggle the watcher and endpoint halves (`--no-watch` / `--no-sync`); `.rustLog` sets `RUST_LOG`.
  The service runs as *you*, using your own identity (`~/.outl`) and device store (`~/.config/outl`) — pairing is plain `outl peer pair`, and the daemon picks up new peers on its own.

> The service runs while you are logged in.
> For a dedicated always-on box (a NAS, a VPS) where nobody sits at a login, enable lingering so your user manager stays alive without a session: `loginctl enable-linger <user>`.
> That is what keeps the box paired 24/7.

### Settings

`programs.outl.settings` mirrors [`outl.toml`](config.md).
A few of the keys:

| Option | Type | Default | Meaning |
|---|---|---|---|
| `theme.preset` | enum | `"outl"` | One of `outl`, `default-dark`, `light`, `logseq-light`, `dracula`, `solarized-dark`, `nord`, `monokai`, `gruvbox`. |
| `editor.vimMode` | bool | `true` | Vim-style modal bindings (desktop). |
| `sync.transport` | enum | `"iroh"` | `"iroh"` (P2P QUIC) or `"file"` (iCloud / shared FS). |
| `reminders.quietHours` | str? | `null` | e.g. `"22:00-07:00"`. |
| `backup.enabled` / `backup.intervalMinutes` | bool / int | `true` / `30` | Automatic git snapshots of the workspace. |
| `extraConfig` | attrs | `{}` | Merged last, for keys the module does not model yet. |

The full key list is in the module source ([`hm-module.nix`](../hm-module.nix)); the meaning of each key is in [Configuration](config.md).

## Which do I use?

| Situation | Use |
|---|---|
| Just want the CLI/TUI on any Nix machine | `packages.outl` |
| Want `config.toml` managed + optional logged-in sync on a laptop/desktop | `homeManagerModules.default` |
| A dedicated box that must stay paired 24/7 (NAS, VPS) | `homeManagerModules.default` + `loginctl enable-linger <user>` |
