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

          desktopVersion = "0.12.0-beta.168";

          outl-desktop =
            let
              isLinux = pkgs.stdenv.hostPlatform.isLinux;
              isDarwin = pkgs.stdenv.hostPlatform.isDarwin;
            in
            if isLinux then
              let
                appimage = pkgs.fetchurl {
                  url = "https://github.com/outlmd/outl/releases/download/v${desktopVersion}/outl-desktop-linux-x86_64.AppImage";
                  hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
                };
              in
              pkgs.appimageTools.wrapType2 {
                name = "outl-desktop";
                src = appimage;
                meta = with pkgs.lib; {
                  description = "Desktop client for outl (Tauri 2)";
                  homepage = "https://outl.app";
                  license = licenses.mit;
                  platforms = platforms.linux;
                  mainProgram = "outl-desktop";
                };
              }
            else if isDarwin then
              pkgs.stdenv.mkDerivation {
                pname = "outl-desktop";
                version = desktopVersion;

                src = pkgs.fetchurl {
                  url = "https://github.com/outlmd/outl/releases/download/v${desktopVersion}/outl-desktop-macos.dmg";
                  hash = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
                };

                nativeBuildInputs = with pkgs; [
                  undmg
                ];

                unpackPhase = ''
                  undmg $src
                '';

                installPhase = ''
                  mkdir -p $out/Applications
                  cp -r outl.app $out/Applications/
                '';

                meta = with pkgs.lib; {
                  description = "Desktop client for outl (Tauri 2)";
                  homepage = "https://outl.app";
                  license = licenses.mit;
                  platforms = platforms.darwin;
                };
              }
            else
              throw "outl-desktop is not supported on this platform";
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
