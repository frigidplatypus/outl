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
    settings.theme.preset = "dracula";     # light side; see "Theme precedence"
    settings.theme.presetDark = "dracula"; # the TUI renders this side
    settings.editor.vimMode = true;
    services.sync = { enable = true; workspace = "~/notes"; };
  };
};
```

`programs.outl.settings.*` mirrors the upstream `config.toml` schema — one option per field.
See `hm-module.nix` for the exact list; each option carries a `description` that home-manager renders as documentation (`programs.outl.services.sync` runs `outl serve` as a systemd user service that holds this device's iroh endpoint; `installDesktop = true` also installs the Tauri GUI).
`programs.outl.extraConfig` is a deep-merged escape hatch for any field the module has not modelled yet — write its keys in `snake_case`, matching `config.toml`.

### Theme precedence

The theme is the one setting whose effect is easy to misread, because outl resolves it through **two layers this module does not control**:

1. **Global vs. workspace.**
   This module writes the **global** `~/.config/outl/config.toml`, which is outl's *lowest* theme precedence.
   Precedence is first-hit-wins: `--theme <preset>` → a per-workspace `<root>/.outl/config.toml` `[theme] preset` → the global file → the built-in default.
   So if a workspace's own `.outl/config.toml` carries a `[theme] preset` (it survives ordinary config rewrites), **that wins and the home-manager value is ignored** for that workspace — the classic "I set `preset` in Nix but the terminal still shows a different theme" case.
   To let home-manager own a workspace's theme, that workspace's `.outl/config.toml` must have **no `[theme]` section**.

2. **Light/dark pair vs. the terminal.**
   `preset` is the **light** side; `presetDark` is the **dark** side.
   A terminal cannot read OS appearance, so the TUI renders the **dark** side whenever `mode` is `"auto"` (the default) or `"dark"`.
   A lone `settings.theme.preset = "x"` therefore themes the desktop's light side only — the TUI keeps rendering `presetDark` (default `"outl"`).
   To theme the TUI, set `presetDark = "x"` (or `mode = "light"`).
   The module emits an eval-time warning when it sees `preset` customised while `presetDark` is still the untouched default, precisely because that combination silently does nothing on the terminal.

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
