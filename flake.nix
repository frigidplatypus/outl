# Nix flake for outl — consumed directly as a flake input. Full guide: FLAKE.md.
#
#   inputs.outl.url = "github:frigidplatypus/outl";
#   nix run github:frigidplatypus/outl        # -> .#outl (stock upstream CLI/TUI/MCP)
#
# Packages (Linux x86_64 / aarch64):  outl, outl-desktop (stock upstream via the
# pinned `upstream` input) and outl-dev, outl-desktop-dev (this branch's tree).
# Home-manager: outl.homeManagerModules.default (see hm-module.nix).
#
# The default outputs always build the pinned `upstream` input, so they are stock
# outl regardless of which branch of this repo the flake is read from.
{
  description = "outl - local-first outliner with CRDT sync (Nix flake + home-manager module for the upstream release)";

  nixConfig = {
    extra-substituters = [
      "https://outl.cachix.org"
    ];
    extra-trusted-public-keys = [
      "outl.cachix.org-1:xHVg/Xb+czttv9YGNHVlyi2YDZu/XAPQK1o2OUgjuqg="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";

    # Upstream release source, tracked on `main`. Advanced deliberately with
    # `nix flake lock --update-input upstream`. `flake = false` fetches it as a
    # plain source tree rather than evaluating its own flake.
    upstream = {
      url = "github:outlmd/outl/main";
      flake = false;
    };
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
      upstream,
    }:
    # Packages are built for Linux only. macOS users get config-file
    # management via homeManagerModules (see hm-module.nix) but no Nix-built
    # package: the Tauri desktop build needs Apple SDK frameworks that
    # current nixpkgs no longer exposes as stable attributes.
    #
    # Default outputs (`outl`, `outl-desktop`) build the UPSTREAM release from
    # the `upstream` input, so `nix build .#outl` and the home-manager module
    # always yield stock outl regardless of which branch this flake is read
    # from. The `-dev` outputs build `self` — the checked-out tree — and are
    # what a fork/dev ref installs to get local changes on a machine.
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
      ]
      (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };

          # Development toolchain + source (the checked-out tree).
          devToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          version =
            (builtins.fromTOML (builtins.readFile ./Cargo.toml)).workspace.package.version or "0.12.0";
          projectSrc = pkgs.lib.cleanSourceWith {
            src = self;
            filter =
              path: type:
              let
                name = pkgs.lib.baseNameOf path;
              in
              !builtins.elem name [
                ".git"
                ".github"
                "devenv.lock"
                "devenv.nix"
                "devenv.yaml"
                "flake.lock"
                "flake.nix"
                "hm-module.nix"
                "target"
              ];
          };

          # Upstream release source + its own pinned toolchain and version.
          upstreamSrc = upstream;
          upstreamToolchain = pkgs.rust-bin.fromRustupToolchainFile (upstreamSrc + "/rust-toolchain.toml");
          upstreamVersion =
            (builtins.fromTOML (builtins.readFile (upstreamSrc + "/Cargo.toml"))).workspace.package.version
              or "0.12.0";

          linuxDeps = with pkgs; [
            webkitgtk_4_1
            gtk3
            cairo
            gdk-pixbuf
            glib
            dbus
            openssl_3
            libsoup_3
            librsvg
            libappindicator-gtk3
          ];

          mkOutl =
            {
              pname,
              version,
              src,
              cargoLockFile,
              rust,
            }:
            pkgs.rustPlatform.buildRustPackage {
              inherit pname version src;

              cargoLock.lockFile = cargoLockFile;

              nativeBuildInputs = with pkgs; [
                rust
                pkg-config
              ];

              # The CLI and TUI are terminal apps with no system-library
              # dependency: sync uses rustls (never openssl/native-tls) and the
              # TUI is not Tauri, so the GTK/WebKit closure the desktop needs is
              # irrelevant here.
              buildInputs = [ ];

              cargoBuildFlags = [
                "-p"
                "outl-cli"
                "-p"
                "outl-tui"
              ];

              doCheck = false;

              meta = with pkgs.lib; {
                description = "Local-first outliner with CRDT sync";
                homepage = "https://outl.app";
                license = licenses.mit;
                mainProgram = "outl";
                platforms = platforms.linux;
              };
            };

          mkDesktopFrontend =
            {
              pname,
              version,
              src,
              outputHash,
            }:
            pkgs.stdenv.mkDerivation {
              inherit pname version src;

              nativeBuildInputs = with pkgs; [
                bun
                nodejs
              ];

              # Fixed-output derivation: update hash when the frontend source
              # changes. Run the build and copy the `got: sha256-...` value
              # from the mismatch error into `outputHash`.
              outputHashMode = "recursive";
              outputHashAlgo = "sha256";
              inherit outputHash;

              buildPhase = ''
                export HOME=$TMPDIR
                cd crates/outl-desktop
                bun install --frozen-lockfile
                # `bun run build` execs node_modules/.bin/vite, whose shebang is
                # `#!/usr/bin/env node` and the Nix sandbox has no /usr/bin/env,
                # so it dies with "bad interpreter". bun materialises .bin entries
                # as symlinks whose targets may live in its global cache OUTSIDE
                # node_modules (what the GitHub runners hit; a warm local cache
                # links inside the tree, which is why the same derivation builds
                # locally yet fails in CI). patchShebangs skips symlinks and, when
                # the target is out of tree, never reaches it. Resolve each .bin
                # symlink to its real file and patch THAT in place -- its path is
                # preserved, so the script's own relative imports (vite resolves
                # ../dist via the symlink's realpath) still work. installPhase only
                # copies dist/, so the output hash is unaffected.
                for f in node_modules/.bin/*; do
                  if [ -L "$f" ]; then
                    t="$(readlink -f "$f")"
                    [ -w "$t" ] || chmod u+w "$t"
                    patchShebangs "$t"
                  fi
                done
                patchShebangs --build node_modules
                bun run build
              '';

              installPhase = ''
                mkdir -p $out
                cp -r dist/* $out/
              '';
            };

          mkOutlDesktop =
            {
              pname,
              version,
              src,
              cargoLockFile,
              rust,
              frontend,
              features,
            }:
            pkgs.rustPlatform.buildRustPackage {
              inherit pname version src;

              cargoLock.lockFile = cargoLockFile;

              buildAndTestSubdir = "crates/outl-desktop/src-tauri";

              # Plain `cargo build` (not the tauri CLI) leaves Tauri in dev mode,
              # so the webview would load localhost:1421. The webview must be
              # pointed at the embedded `frontendDist` instead, which means
              # enabling `tauri`'s `custom-protocol` feature. We name it directly
              # (not the fork's `production` alias) so the same flake evaluates on
              # an upstream tree, which has no `[features]` section of its own.
              cargoBuildFlags = [
                "--features"
                (builtins.concatStringsSep "," features)
              ];

              nativeBuildInputs = with pkgs; [
                rust
                pkg-config
                makeWrapper
                wrapGAppsHook3
                gobject-introspection
                desktop-file-utils
                xdg-utils
                jq
              ];

              buildInputs = linuxDeps;

              preBuild = ''
                mkdir -p crates/outl-desktop/dist
                cp -r ${frontend}/* crates/outl-desktop/dist/
              '';

              # Plain `cargo build` skips Tauri's bundler, which normally emits
              # the .desktop entry + hicolor icons. Emit them here into the
              # standard XDG paths, so NixOS / home-manager pick them up for the
              # launcher and the .md/.txt "Open With" menu automatically. Name /
              # id / MIME types are read from tauri.conf.json at *build* time,
              # never eval time: interpolating `${src}` only embeds the source
              # path as a string (pure-eval safe), whereas `builtins.readFile`
              # would realise `src`, which pure `nix flake check` refuses to do
              # for `outl-desktop-dev` (whose `src` is the flake's own tree).
              postInstall = ''
                mkdir -p $out/share/applications

                SRC=${src}/crates/outl-desktop/src-tauri
                ID=$(jq -r .identifier "$SRC/tauri.conf.json")
                NAME=$(jq -r .productName "$SRC/tauri.conf.json")
                MIME=$(jq -r '[(.bundle.fileAssociations // [])[].mimeType] | join(";") + ";"' "$SRC/tauri.conf.json")

                cat > "$out/share/applications/$ID.desktop" <<DESKTOP
                [Desktop Entry]
                Type=Application
                Version=1.0
                Name=$NAME
                GenericName=Outliner
                Comment=Local-first outliner with CRDT sync
                Exec=$out/bin/outl-desktop %u
                Icon=$ID
                Terminal=false
                StartupNotify=true
                Categories=Utility;TextEditor;
                Keywords=outline;notes;journal;markdown;outliner;
                StartupWMClass=outl-desktop
                MimeType=$MIME
                DESKTOP

                desktop-file-validate "$out/share/applications/$ID.desktop"

                install -Dm644 "$SRC/icons/32x32.png"     "$out/share/icons/hicolor/32x32/apps/$ID.png"
                install -Dm644 "$SRC/icons/128x128.png"    "$out/share/icons/hicolor/128x128/apps/$ID.png"
                install -Dm644 "$SRC/icons/128x128@2x.png" "$out/share/icons/hicolor/256x256/apps/$ID.png"
                install -Dm644 "$SRC/icons/icon.svg"       "$out/share/icons/hicolor/scalable/apps/$ID.svg"

                wrapProgram $out/bin/outl-desktop \
                  --prefix GST_PLUGIN_PATH : "$GST_PLUGIN_PATH" \
                  --prefix GI_TYPELIB_PATH : "$GI_TYPELIB_PATH" \
                  --prefix PATH : "${pkgs.desktop-file-utils}/bin:${pkgs.xdg-utils}/bin"
              '';

              doCheck = false;

              meta = with pkgs.lib; {
                description = "Desktop client for outl (Tauri 2)";
                homepage = "https://outl.app";
                license = licenses.mit;
                mainProgram = "outl-desktop";
                platforms = platforms.linux;
              };
            };

          # ---- Default outputs: the upstream release ----
          outl = mkOutl {
            pname = "outl";
            version = upstreamVersion;
            src = upstreamSrc;
            cargoLockFile = upstreamSrc + "/Cargo.lock";
            rust = upstreamToolchain;
          };

          desktopFrontendUpstream = mkDesktopFrontend {
            pname = "outl-desktop-frontend";
            version = upstreamVersion;
            src = upstreamSrc;
            # Stock frontend hash. Kept separate from the dev tree's hash
            # below: the dev tree carries frontend changes that upstream has
            # not taken yet, so the two `bun run build` outputs differ. When
            # either side moves, rebuild and copy the `got: sha256-...` value
            # from the FOD mismatch error into the matching hash here.
            outputHash = "sha256-oi6tiLpbxGeyjAQ4D5vBkLrzx3cDRisYMbRRYey9RgU=";
          };

          outl-desktop = mkOutlDesktop {
            pname = "outl-desktop";
            version = upstreamVersion;
            src = upstreamSrc;
            cargoLockFile = upstreamSrc + "/Cargo.lock";
            rust = upstreamToolchain;
            frontend = desktopFrontendUpstream;
            features = [ "tauri/custom-protocol" ];
          };

          # ---- Development outputs: the checked-out tree ----
          outl-dev = mkOutl {
            pname = "outl";
            inherit version;
            src = projectSrc;
            cargoLockFile = ./Cargo.lock;
            rust = devToolchain;
          };

          desktopFrontendDev = mkDesktopFrontend {
            pname = "outl-desktop-frontend";
            inherit version;
            src = projectSrc;
            outputHash = "sha256-bH3dyEP82lURXw9O/AtKiXtYzF/3VMyhTjwrN1fKIxQ=";
          };

          outl-desktop-dev = mkOutlDesktop {
            pname = "outl-desktop";
            inherit version;
            src = projectSrc;
            cargoLockFile = ./Cargo.lock;
            rust = devToolchain;
            frontend = desktopFrontendDev;
            features = [ "tauri/custom-protocol" ];
          };
        in
        {
          packages = {
            inherit
              outl
              outl-desktop
              outl-dev
              outl-desktop-dev
              ;
            # The frontend fixed-output derivations, exposed so CI can build
            # just the JS bundle (a few seconds) to validate its `outputHash`
            # before advancing main, instead of paying for the full desktop
            # Rust build to discover a stale hash.
            outl-desktop-frontend = desktopFrontendUpstream;
            outl-desktop-frontend-dev = desktopFrontendDev;
            default = outl;
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [
              outl-dev
              outl-desktop-dev
            ];
            packages = with pkgs; [
              devToolchain
              cargo-tauri
              bun
              nodejs
              just
            ];
          };

          formatter = pkgs.nixfmt;
        }
      )
    // {
      homeManagerModules.default = import ./hm-module.nix self;
    };
}
