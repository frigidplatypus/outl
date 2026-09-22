{
  description = "outl - local-first outliner with CRDT sync";

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
          version = "0.12.0";
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

              buildInputs = with pkgs; [
                glib
                gtk3
                webkitgtk_4_1
                dbus
                openssl_3
              ];

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
              ];

              buildInputs = linuxDeps;

              preBuild = ''
                mkdir -p crates/outl-desktop/dist
                cp -r ${frontend}/* crates/outl-desktop/dist/
              '';

              postInstall = ''
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
            # Upstream's frontend currently builds byte-identically to the dev
            # tree. When the upstream frontend diverges, rebuild and replace
            # this with the `got: sha256-...` value from the FOD mismatch error.
            outputHash = "sha256-omoPnNTZnfCP0Ed+xHjubjDdTPelt0ppZFs8XCWFq+E=";
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
            outputHash = "sha256-omoPnNTZnfCP0Ed+xHjubjDdTPelt0ppZFs8XCWFq+E=";
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

          formatter = pkgs.nixfmt-rfc-style;
        }
      )
    // {
      homeManagerModules.default = import ./hm-module.nix self;
    };
}
