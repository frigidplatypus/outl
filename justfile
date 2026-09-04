# Local build commands for outl.
#
# `build` / `build-desktop` use native cargo — incremental, reuses target/,
# the fast loop for day-to-day work.
# `nix-build` / `push` use the hermetic flake builds (store paths) and the
# outl cachix cache (https://outl.cachix.org).

# Native build of the CLI + TUI (same crates as the `outl` nix package).
# Debug by default so it reuses the warm target/debug cache; pass --release
# for a release build: `just build --release`
build *flags="":
    cargo build {{flags}} -p outl-cli -p outl-tui

# Desktop production bundle (Tauri, embeds the Vite frontend).
build-desktop:
    cd crates/outl-desktop && cargo tauri build

# Hermetic nix build of both packages.
nix-build:
    nix build .#outl .#outl-desktop

# Nix build both packages and push them to the outl cachix cache.
push:
    cachix push outl $(nix build .#outl .#outl-desktop --print-out-paths)
