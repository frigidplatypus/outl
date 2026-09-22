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

## Binary cache

The flake advertises the shared outl Cachix cache in its `nixConfig`:

```
extra-substituters          = https://outl.cachix.org
extra-trusted-public-keys   = outl.cachix.org-1:xHVg/Xb+czttv9YGNHVlyi2YDZu/XAPQK1o2OUgjuqg=
```

Nix only *applies* a flake's `nixConfig` if you let it — otherwise it is silently ignored.
Either set `accept-flake-config = true` in your `nix.conf`, or add the substituter and key to `nix.conf` yourself.
Without one of those you will still get a working build; it just compiles from source instead of downloading a cached one.

Prebuilt binaries are published to this cache by `.github/workflows/nix-build.yml` on every push to `main` (and via the `push` recipe in the `justfile`).
The cache holds whatever has been pushed to it — it is populated as releases are published, so a freshly bumped `upstream` revision may not have a prebuilt binary yet and will fall back to a local build.

## Home-manager module

```nix
inputs.outl.url = "github:frigidplatypus/outl";

homeConfigurations.me = {
  modules = [ inputs.outl.homeManagerModules.default ];

  programs.outl = {
    enable = true;
    settings.theme.preset = "dracula";  # themes the terminal AND the desktop
    settings.editor.vimMode = true;
    services.sync = { enable = true; workspace = "~/notes"; };
  };
};
```

`programs.outl.settings.*` mirrors the upstream `config.toml` schema — one option per field.
See `hm-module.nix` for the exact list; each option carries a `description` that home-manager renders as documentation (`programs.outl.services.sync` runs `outl serve` as a systemd user service that holds this device's iroh endpoint; `installDesktop = true` also installs the Tauri GUI).
`programs.outl.extraConfig` is a deep-merged escape hatch for any field the module has not modelled yet — write its keys in `snake_case`, matching `config.toml`.

### Theme precedence

The theme is the one setting whose effect is easy to misread, because outl stores a **light/dark pair** and resolves it through layers this module does not fully control.

1. **Light/dark pair vs. the terminal.**
   `preset` is the light side; `presetDark` is the dark side.
   A terminal cannot read OS appearance, so the TUI renders the **dark** side whenever `mode` is `"auto"` (the default) or `"dark"`.
   This module makes `presetDark` default to *following* `preset`, so a single `settings.theme.preset = "x"` themes the terminal and the desktop together.
   Set `presetDark` explicitly only when you want the two sides to differ — for example a custom light side with the brand `"outl"` dark theme.
   When you customise neither, the module keeps outl's brand pair `outl-light` / `outl`, so the terminal defaults to the brand dark theme rather than to the light preset.

2. **Global vs. workspace (less common).**
   This module writes the **global** `~/.config/outl/config.toml`, which is outl's *lowest* theme precedence.
   Precedence is first-hit-wins: `--theme <preset>` → a per-workspace `<root>/.outl/config.toml` `[theme] preset` → the global file → the built-in default.
   A default `outl init` workspace has **no** `[theme]` section, so home-manager's global value applies everywhere.
   Only a workspace whose `.outl/config.toml` you have hand-added a `[theme] preset` to **overrides** home-manager for that workspace — the "I set `preset` in Nix but this one folder still shows another theme" case.

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
