flake:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.outl;

  tomlFormat = pkgs.formats.toml { };

  themePresets = [
    "outl"
    "default-dark"
    "light"
    "logseq-light"
    "dracula"
    "solarized-dark"
    "nord"
    "monokai"
    "gruvbox"
  ];
in
{
  options.programs.outl = {
    enable = lib.mkEnableOption "outl outliner";

    package = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.system}.outl;
      defaultText = lib.literalExpression "outl flake's package";
      description = "The outl package to use.";
    };

    desktopPackage = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.system}.outl-desktop;
      defaultText = lib.literalExpression "outl flake's outl-desktop package";
      description = "The outl-desktop package to use.";
    };

    installDesktop = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Whether to also install the outl-desktop Tauri application.";
    };

    services.sync = {
      enable = lib.mkEnableOption "outl background sync service";

      workspace = lib.mkOption {
        type = lib.types.str;
        description = "Path to the workspace to sync.";
      };
    };

    settings = lib.mkOption {
      type = lib.types.submodule {
        options = {
          workspace = {
            last = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "Absolute path to the last workspace opened.";
            };
          };

          theme = {
            preset = lib.mkOption {
              type = lib.types.enum themePresets;
              default = "outl";
              description = "Theme palette preset.";
            };
          };

          editor = {
            vimMode = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Enable vim-style modal bindings in the desktop client.";
            };

            fontSize = lib.mkOption {
              type = lib.types.int;
              default = 15;
              description = "Outline font size in pixels (desktop only).";
            };
          };

          calendar = {
            timezone = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "IANA timezone name (e.g. 'Europe/London'). Uses OS timezone when unset.";
            };
          };

          sync = {
            transport = lib.mkOption {
              type = lib.types.enum [
                "iroh"
                "file"
              ];
              default = "iroh";
              description = "Sync transport: 'iroh' for P2P QUIC, 'file' for iCloud/shared FS.";
            };

            relayUrl = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              description = "Custom iroh relay URL. Uses outl's default relay when unset.";
            };
          };

          display = {
            backlinksOrder = lib.mkOption {
              type = lib.types.enum [
                "newest"
                "oldest"
              ];
              default = "newest";
              description = "Sort direction for backlinks list.";
            };
          };

          assets = {
            maxBytes = lib.mkOption {
              type = lib.types.int;
              default = 104857600;
              description = "Maximum size in bytes for a single uploaded file. 0 = unbounded.";
            };
          };

          reminders = {
            enabled = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Whether this device delivers reminder notifications.";
            };

            quietHours = lib.mkOption {
              type = lib.types.nullOr lib.types.str;
              default = null;
              example = "22:00-07:00";
              description = "Time window where reminders are deferred (e.g. '22:00-07:00').";
            };
          };

          snapshot = {
            enabled = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Enable materialized-state snapshots for faster boot.";
            };

            opThreshold = lib.mkOption {
              type = lib.types.int;
              default = 10000;
              description = "Number of ops between snapshot writes.";
            };
          };

          storage = {
            lruCap = lib.mkOption {
              type = lib.types.int;
              default = 20000;
              description = "Maximum ops held in memory. 0 = unbounded.";
            };
          };

          tui = {
            mouseCapture = lib.mkOption {
              type = lib.types.bool;
              default = false;
              description = "Capture mouse events in TUI (disables terminal text selection).";
            };
          };

          backup = {
            enabled = lib.mkOption {
              type = lib.types.bool;
              default = true;
              description = "Enable automatic git snapshots of the workspace.";
            };

            intervalMinutes = lib.mkOption {
              type = lib.types.int;
              default = 30;
              description = "Minimum minutes between automatic snapshots.";
            };
          };

          extraConfig = lib.mkOption {
            type = lib.types.attrsOf lib.types.anything;
            default = { };
            example = lib.literalExpression ''
              {
                custom_section = {
                  key = "value";
                };
              }
            '';
            description = "Additional configuration to merge into the generated config.toml. Use for fields not yet modeled by this module.";
          };
        };
      };
      default = { };
      description = "outl configuration. See https://outl.app/docs/config";
    };
  };

  config = lib.mkIf cfg.enable (
    let
      configData =
        let
          s = cfg.settings;
        in
        {
          workspace = lib.filterAttrs (_: v: v != null) {
            last = s.workspace.last;
          };

          theme = {
            preset = s.theme.preset;
          };

          editor = {
            vim_mode = s.editor.vimMode;
            font_size = s.editor.fontSize;
          };

          calendar = lib.filterAttrs (_: v: v != null) {
            timezone = s.calendar.timezone;
          };

          sync = lib.filterAttrs (_: v: v != null) {
            transport = s.sync.transport;
            relay_url = s.sync.relayUrl;
          };

          display = {
            backlinks_order = s.display.backlinksOrder;
          };

          assets = {
            max_bytes = s.assets.maxBytes;
          };

          reminders = {
            enabled = s.reminders.enabled;
          } // lib.filterAttrs (_: v: v != null) {
            quiet_hours = s.reminders.quietHours;
          };

          snapshot = {
            enabled = s.snapshot.enabled;
            op_threshold = s.snapshot.opThreshold;
          };

          storage = {
            lru_cap = s.storage.lruCap;
          };

          tui = {
            mouse_capture = s.tui.mouseCapture;
          };

          backup = {
            enabled = s.backup.enabled;
            interval_minutes = s.backup.intervalMinutes;
          };
        } // s.extraConfig;

      configFile = tomlFormat.generate "outl-config" configData;
    in
    {
      # Packages are only built for Linux (see flake.nix). On other platforms
      # we still generate the config file below, but install nothing — users
      # there provide outl themselves (e.g. the official installer).
      home.packages = lib.optionals pkgs.stdenv.isLinux ([
        cfg.package
      ] ++ lib.optional cfg.installDesktop cfg.desktopPackage);

      xdg.configFile."outl/config.toml".source = configFile;

      systemd.user.services.outl-sync = lib.mkIf (cfg.services.sync.enable && pkgs.stdenv.isLinux) {
        Unit = {
          Description = "outl background sync service";
          After = [ "network-online.target" ];
          Wants = [ "network-online.target" ];
        };

        Service = {
          ExecStart = "${cfg.package}/bin/outl serve --workspace ${cfg.services.sync.workspace}";
          Restart = "on-failure";
          RestartSec = "5s";
        };

        Install = {
          WantedBy = [ "default.target" ];
        };
      };
    }
  );
}
