# Nix

outl is a [flake](../flake.nix).
One input exposes three surfaces: a set of **packages**, a **home-manager module** for per-user configuration, and a **NixOS module** for an always-on sync peer.

| Flake output | Source | What it is |
|---|---|---|
| `packages.outl` *(default)* | [`flake.nix`](../flake.nix) | The CLI (`outl`) and the TUI (`outl-tui`). |
| `packages.outl-desktop` | [`flake.nix`](../flake.nix) | The Tauri 2 desktop app. |
| `homeManagerModules.default` | [`hm-module.nix`](../hm-module.nix) | `programs.outl` — manages `~/.config/outl/config.toml`, optionally installs the packages and a per-user sync service. |
| `nixosModules.default` | [`nixos-module.nix`](../nixos-module.nix) | `services.outl` — runs `outl serve` as a hardened system service (an always-on peer). |

> **Linux only.**
> Packages build for `x86_64-linux` and `aarch64-linux`.
> There is no Nix-built macOS package: the Tauri desktop build needs Apple SDK frameworks that current nixpkgs no longer exposes as stable attributes.
> On macOS the home-manager module still writes your `config.toml`; you supply the binary yourself (see the [Homebrew tap](homebrew.md)).

## The flake

The flake pins `nixpkgs-unstable`, `flake-utils`, and [`rust-overlay`](https://github.com/oxalica/rust-overlay), which reads the compiler toolchain straight from [`rust-toolchain.toml`](../rust-toolchain.toml).

It also declares a Cachix substituter (`https://outl.cachix.org`, see the `nixConfig` block in [`flake.nix`](../flake.nix)), so installing outl pulls a pre-built binary instead of compiling Rust.
You do not need a Rust toolchain to use any of the three surfaces below.

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

> The home-manager sync service and the NixOS module below both run `outl serve`; they target different machines.
> Use the home-manager one for a laptop or desktop that syncs while you are logged in.
> Use the NixOS module for a dedicated always-on box (a NAS, a VPS) that must stay paired even when nobody is logged in.

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

## NixOS module

[`nixos-module.nix`](../nixos-module.nix) is exposed as `nixosModules.default` under the `services.outl` namespace.
It turns a machine into an **always-on outl peer** — the systemd equivalent of the [Docker setup](self-hosting.md).
It is one more peer, exactly like a laptop or phone, except it never sleeps.

```nix
# configuration.nix
{
  imports = [
    (builtins.getFlake "github:outlmd/outl").nixosModules.default
  ];

  networking.hostName = "nas";

  services.outl = {
    enable = true;
    name = "nas";                  # label shown in peers' `outl peer list`
    workspace = "/var/lib/outl";   # the notes — replicated to every peer
    deviceDir = "/var/lib/outl-device";  # this box's identity — NOT replicated
  };
}
```

Two state directories, kept separate on purpose:

- **`workspace`** (`/var/lib/outl`) — the notes: `pages/`, `journals/`, `ops/`, `.outl/`.
  This is what syncs to every paired device.
- **`deviceDir`** (`/var/lib/outl-device`) — the iroh identity (`identity.key`, which *is* this device's node id) and the actor bindings.
  It must live **outside** the workspace; losing it makes the box come back as a new, unpaired device.

The module creates a system user `outl` and two units:

- **`outl-serve`** — the daemon.
  Runs as `outl` with `HOME`/`XDG_CONFIG_HOME` under `deviceDir` and `OUTL_WORKSPACE` pointing at the workspace.
  Hardened (`ProtectSystem=strict`, `ReadWritePaths` limited to the two state dirs), restarts on failure, and gets a 30 s stop grace so it can release its endpoint lease cleanly.
  It creates a `--bare` workspace on first boot if none exists.
- **`outl-pair`** — an enabled one-shot that is a no-op until you place a
  ticket: it runs at boot and exits 0 when there is nothing to do. You run it
  once to join an existing graph (below). It is deliberately not disabled —
  NixOS masks disabled units, which would make the manual start impossible.

### Joining an existing graph

The box **joins**; it never hosts the pairing.
Full walkthrough in [Self-hosting → Step 4](self-hosting.md).

1. On an existing device, mint a ticket: `outl peer invite`.
2. On the new box, either pipe it straight in:

   ```bash
   echo "<ticket>" | sudo -u outl env \
     HOME=/var/lib/outl-device \
     XDG_CONFIG_HOME=/var/lib/outl-device/.config \
     OUTL_WORKSPACE=/var/lib/outl \
     outl peer pair --ticket - --name nas
   ```

   or drop it in the device dir and let the unit do it:

   ```bash
   echo "<ticket>" | sudo tee /var/lib/outl-device/.pair-ticket >/dev/null
   sudo chmod 600 /var/lib/outl-device/.pair-ticket
   sudo systemctl start outl-pair
   ```

The ticket is single-use and short-lived (~120 s).
`outl-pair` hands it to `outl peer pair --ticket -` on stdin (so it never appears in `ps` or a world-readable unit file), then deletes it.
Re-running it is a no-op once `peers.json` is non-empty.

## Which do I use?

| Situation | Use |
|---|---|
| Just want the CLI/TUI on any Nix machine | `packages.outl` |
| Want `config.toml` managed + optional logged-in sync on a laptop/desktop | `homeManagerModules.default` |
| A dedicated box that must stay paired 24/7 (NAS, VPS) | `nixosModules.default` |

They are independent — nothing stops you combining the home-manager module (for config) with the NixOS module (for the daemon) on the same machine.
