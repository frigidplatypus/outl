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

          commonRustArgs = {
            nativeBuildInputs = with pkgs; [
              rustToolchain
              pkg-config
            ];
          };

          outl = pkgs.rustPlatform.buildRustPackage rec {
            pname = "outl";
            version = "0.12.0";

            src = self;

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = commonRustArgs.nativeBuildInputs;

            buildInputs = pkgs.lib.optionals pkgs.stdenv.isLinux (with pkgs; [
              glib
              dbus
              openssl_3
            ]);

            cargoBuildFlags = [
              "-p"
              "outl-cli"
              "-p"
              "outl-tui"
            ];

            meta = with pkgs.lib; {
              description = "Local-first outliner with CRDT sync";
              homepage = "https://outl.app";
              license = licenses.mit;
              mainProgram = "outl";
            };
          };

          desktopFrontend = pkgs.stdenv.mkDerivation {
            pname = "outl-desktop-frontend";
            version = "0.12.0";

            src = self;

            nativeBuildInputs = with pkgs; [
              bun
              nodejs
            ];

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
              isLinux = pkgs.stdenv.isLinux;
              isDarwin = pkgs.stdenv.isDarwin;
            in
            pkgs.rustPlatform.buildRustPackage rec {
              pname = "outl-desktop";
              version = "0.12.0";

              src = self;

              cargoLock.lockFile = ./Cargo.lock;

              buildAndTestSubdir = "crates/outl-desktop/src-tauri";

              nativeBuildInputs =
                commonRustArgs.nativeBuildInputs
                ++ (with pkgs; [
                  bun
                  nodejs
                  makeWrapper
                ])
                ++ pkgs.lib.optionals isLinux (
                  with pkgs;
                  [
                    wrapGAppsHook3
                    gobject-introspection
                  ]
                );

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
                export HOME=$TMPDIR
                cd crates/outl-desktop
                mkdir -p dist
                cp -r ${desktopFrontend}/* dist/
                cd ../..
              '';

              postFixup = pkgs.lib.optionalString isLinux ''
                wrapProgram $out/bin/outl-desktop \
                  --prefix GST_PLUGIN_PATH : "$GST_PLUGIN_PATH" \
                  --prefix GI_TYPELIB_PATH : "$GI_TYPELIB_PATH"
              '';

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
            inputsFrom = [ outl ];
            packages = with pkgs; [
              bun
              nodejs
            ];
          };
        }
      )
    // {
      homeManagerModules.default = import ./hm-module.nix self;
    };
}
