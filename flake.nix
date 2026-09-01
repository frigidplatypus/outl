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
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
      rust-overlay,
    }:
      # Packages are built for Linux only. macOS users get config-file
      # management via homeManagerModules (see hm-module.nix) but no Nix-built
      # package: the Tauri desktop build needs Apple SDK frameworks that
      # current nixpkgs no longer exposes as stable attributes.
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
          rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          version = "0.12.0";
          projectSrc = pkgs.lib.cleanSourceWith {
            src = self;
            filter = path: type:
              let
                name = pkgs.lib.baseNameOf path;
              in
              !builtins.elem name [
                ".github"
                "devenv.lock"
                "devenv.nix"
                "devenv.yaml"
                "flake.lock"
                "flake.nix"
                "hm-module.nix"
                "nixos-module.nix"
              ];
          };

          commonRustArgs = {
            nativeBuildInputs = with pkgs; [
              rustToolchain
              pkg-config
            ];
          };

          outl = pkgs.rustPlatform.buildRustPackage {
            pname = "outl";
            inherit version;

            src = projectSrc;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = commonRustArgs.nativeBuildInputs;

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

          desktopFrontend = pkgs.stdenv.mkDerivation {
            pname = "outl-desktop-frontend";
            inherit version;

            src = projectSrc;

            nativeBuildInputs = with pkgs; [
              bun
              nodejs
            ];

            # Fixed-output derivation: update hash when frontend code changes
            # Run build to get the correct hash from the error message
            outputHashMode = "recursive";
            outputHashAlgo = "sha256";
            outputHash = "sha256-Tpy7kbULEd4IFTpkfVf0gQOc+JohjXxVm1vlGe10BOY=";

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

          outl-desktop = pkgs.rustPlatform.buildRustPackage {
            pname = "outl-desktop";
            inherit version;

            src = projectSrc;

            cargoLock.lockFile = ./Cargo.lock;

            buildAndTestSubdir = "crates/outl-desktop/src-tauri";

            # Plain `cargo build` (not the tauri CLI) leaves Tauri in dev mode,
            # so the webview would load localhost:1421. Enable the production
            # feature to embed the frontend instead.
            cargoBuildFlags = [ "--features" "production" ];

            nativeBuildInputs = with pkgs; [
              rustToolchain
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
              cp -r ${desktopFrontend}/* crates/outl-desktop/dist/
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
        in
        {
          packages = {
            inherit outl outl-desktop;
            default = outl;
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ outl outl-desktop ];
            packages = with pkgs; [
              rustToolchain
              cargo-tauri
              bun
              nodejs
            ];
          };

          formatter = pkgs.nixfmt-rfc-style;
        }
      )
    // {
      homeManagerModules.default = import ./hm-module.nix self;
      nixosModules.default = import ./nixos-module.nix self;
    };
}
