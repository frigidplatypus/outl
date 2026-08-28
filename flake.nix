{
  description = "outl - local-first outliner with CRDT sync";

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
    flake-utils.lib.eachSystem
      [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
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

          commonRustArgs = {
            nativeBuildInputs = with pkgs; [
              rustToolchain
              pkg-config
            ];
          };

          outl = pkgs.rustPlatform.buildRustPackage {
            pname = "outl";
            inherit version;

            src = self;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = commonRustArgs.nativeBuildInputs;

            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux (with pkgs; [
              glib
              gtk3
              webkitgtk_4_1
              dbus
              openssl_3
            ]);

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

            src = self;

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

          outl-desktop =
            let
              isLinux = pkgs.stdenv.hostPlatform.isLinux;
              isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
            in
            pkgs.rustPlatform.buildRustPackage {
              pname = "outl-desktop";
              inherit version;

              src = self;

              cargoLock.lockFile = ./Cargo.lock;

              buildAndTestSubdir = "crates/outl-desktop/src-tauri";

              nativeBuildInputs = with pkgs; [
                rustToolchain
                pkg-config
                makeWrapper
              ] ++ pkgs.lib.optionals isLinux [
                wrapGAppsHook3
                gobject-introspection
              ];

              buildInputs =
                pkgs.lib.optionals isLinux linuxDeps
                ++ pkgs.lib.optionals isDarwin (
                  with pkgs.darwin.apple_sdk.frameworks;
                  [
                    WebKit
                    AppKit
                    Security
                    SystemConfiguration
                    Cocoa
                    CoreFoundation
                  ]
                );

              preBuild = ''
                mkdir -p crates/outl-desktop/dist
                cp -r ${desktopFrontend}/* crates/outl-desktop/dist/
              '';

              postInstall =
                if isLinux then
                  ''
                    wrapProgram $out/bin/outl-desktop \
                      --prefix GST_PLUGIN_PATH : "$GST_PLUGIN_PATH" \
                      --prefix GI_TYPELIB_PATH : "$GI_TYPELIB_PATH"
                  ''
                else "";

              doCheck = false;

              meta = with pkgs.lib; {
                description = "Desktop client for outl (Tauri 2)";
                homepage = "https://outl.app";
                license = licenses.mit;
                mainProgram = "outl-desktop";
                platforms = platforms.linux ++ platforms.darwin;
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
    };
}
