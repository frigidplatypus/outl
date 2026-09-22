# Nix / home-manager

This repo is a thin packaging overlay on top of
[outlmd/outl](https://github.com/outlmd/outl).
The flake builds the **stock upstream release** by default, so it is a reproducible way to install outl — not a fork of its code.

## Try it

```console
nix run github:frigidplatypus/outl            # outl CLI/TUI/MCP (default output)
nix run github:frigidplatypus/outl#outl-desktop
```

## As a flake input

```nix
inputs.outl.url = "github:frigidplatypus/outl";

# ...
environment.systemPackages = [ inputs.outl.packages.${system}.outl ];
# inputs.outl.packages.${system}.outl-desktop   (Tauri GUI)
```

The default outputs (`outl`, `outl-desktop`, `default`) build the pinned `upstream` input, so they are stock outl no matter which branch this flake is read from.
The `-dev` outputs (`outl-dev`, `outl-desktop-dev`) build *this branch's tree* — reach for them only when the input points at the `dev` ref and you want local changes.

## Home-manager module

```nix
inputs.outl.url = "github:frigidplatypus/outl";

homeConfigurations.me = {
  modules = [ inputs.outl.homeManagerModules.default ];

  programs.outl = {
    enable = true;
    settings.theme.preset = "dracula";     # mirrors outl config.toml
    settings.editor.vimMode = true;
    services.sync = { enable = true; workspace = "~/notes"; };
  };
};
```

`programs.outl.settings.*` mirrors the upstream `config.toml` schema — one option per field.
See `hm-module.nix` for the exact list; each option carries a `description` that home-manager renders as documentation (`programs.outl.services.sync` runs `outl serve` as a systemd user service that holds this device's iroh endpoint; `installDesktop = true` also installs the Tauri GUI).
`programs.outl.extraConfig` is a deep-merged escape hatch for any field the module has not modelled yet — write its keys in `snake_case`, matching `config.toml`.

## Platforms

- **Linux** (`x86_64`, `aarch64`): CLI, TUI, desktop, and the sync service.
- **macOS / darwin**: config-file management only.
  The Tauri build needs Apple SDK frameworks that current nixpkgs no longer exposes as stable attributes, so there is no Nix-built package on darwin.

## Maintainer notes

- **Bump the release:** `nix flake lock --update-input upstream`, then verify the build.
- **Frontend FOD hash:** after a large upstream frontend change the desktop `outputHash` in `flake.nix` may not match.
  Rebuild `.#outl-desktop`, copy the `got: sha256-…` from the FOD mismatch error into both `outputHash` values, and rebuild.
- **Branch model:** `main` is upstream plus this nix overlay.
  Merging `upstream/main` into it stays conflict-free forever, because the overlay only ever adds `flake.nix`, `flake.lock`, `hm-module.nix`, and `FLAKE.md` — filenames upstream does not use.
  `dev` carries the fork's own changes and is where the `-dev` outputs build from.
