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
    "outl-light"
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
      defaultText = lib.literalExpression "outl flake's upstream outl package";
      description = ''
        The outl package to use. The flake default builds the upstream release;
        set it to flake.packages.''${system}.outl-dev to install a fork / dev
        branch build (when the flake input points at that ref).
      '';
    };

    desktopPackage = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.system}.outl-desktop;
      defaultText = lib.literalExpression "outl flake's upstream outl-desktop package";
      description = ''
        The outl-desktop package to use. The flake default builds the upstream
        release; set it to flake.packages.''${system}.outl-desktop-dev for a
        fork / dev branch build.
      '';
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

      watch = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Run the file watcher (reconciles .md written into the workspace from outside into the op log). Disable for a pure endpoint holder: cheaper to leave running beside a GUI or TUI, since it takes no per-actor write lock.";
      };

      sync = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = "Hold this device's iroh endpoint so paired peers converge continuously.";
      };

      rustLog = lib.mkOption {
        type = lib.types.str;
        default = "info";
        example = "outl=debug,iroh=info";
        description = "RUST_LOG for the daemon. Use outl=debug when diagnosing a pairing that will not connect.";
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
              default = "outl-light";
              description = lib.mdDoc ''
                Theme palette preset — the light side of the light/dark pair.
                Names match outl_theme::PRESETS.

                Two caveats on where this lands:

                - This module writes the **global** `~/.config/outl/config.toml`,
                  which is outl's *lowest* theme precedence. A per-workspace
                  `<root>/.outl/config.toml` `[theme] preset` and the `--theme`
                  flag both override it wholesale. For this setting to take
                  effect in a workspace, that workspace's `.outl/config.toml`
                  must carry **no** `[theme]` section.
                - The TUI always renders the **dark** side (`presetDark`) under
                  `mode = "auto"` or `"dark"` — a terminal cannot read OS
                  appearance. So this option alone does not theme the TUI; set
                  `presetDark` (or `mode = "light"`) to do that.
              '';
            };

            presetDark = lib.mkOption {
              type = lib.types.nullOr (lib.types.enum themePresets);
              default = "outl";
              description = lib.mdDoc ''
                Dark side of the pair — the preset the TUI actually renders.
                `null` falls back to `preset` (the pre-RFC-0022 single-preset
                behaviour). Leave at the `"outl"` default only if you want the
                brand dark theme in the terminal; to theme the TUI set this, not
                `preset`.
              '';
            };

            mode = lib.mkOption {
              type = lib.types.enum [
                "light"
                "dark"
                "auto"
              ];
              default = "auto";
              description = lib.mdDoc ''
                Which side of the pair to render. The TUI cannot read OS
                appearance and treats `"auto"` as **dark** (renders
                `presetDark`), so `"auto"` is a desktop setting as far as the
                terminal is concerned.
              '';
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
      # Footgun guard (docs in FLAKE.md → "Theme precedence").
      #
      # The TUI renders `presetDark` under `mode = auto|dark`, so a user who
      # customises `preset` but leaves `presetDark` at its default sees the
      # brand `outl` theme in the terminal and their `preset` silently ignored.
      # Detect that exact shape at eval and warn (non-fatal — a `presetDark`
      # assertion would be wrong, since a custom light-side preset with the
      # brand dark theme is a legitimate choice). Silencing: set `presetDark`
      # explicitly, even back to `"outl"`.
      tc = cfg.settings.theme;
      themeShadowed =
        tc.mode != "light"
        && tc.presetDark == "outl"
        && tc.preset != "outl-light";
      themeShadowedMsg = ''
        programs.outl.settings.theme.preset = "${tc.preset}" does not theme the TUI: with mode = "${tc.mode}" the terminal renders the dark side, `presetDark`, which is still the default "outl". Set programs.outl.settings.theme.presetDark = "${tc.preset}" to theme the terminal, or set mode = "light" to render `preset` everywhere. Set presetDark explicitly (even back to "outl") to silence this warning.
      '';

      configData = lib.warnIf themeShadowed themeShadowedMsg (
        let
          s = cfg.settings;
        in
        lib.recursiveUpdate ({
          workspace = lib.filterAttrs (_: v: v != null) {
            last = s.workspace.last;
          };

          theme = {
            preset = s.theme.preset;
            mode = s.theme.mode;
          }
          // lib.filterAttrs (_: v: v != null) {
            preset_dark = s.theme.presetDark;
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
          }
          // lib.filterAttrs (_: v: v != null) {
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
        }) s.extraConfig);

      configFile = tomlFormat.generate "outl-config" configData;
    in
    {
      # Packages are only built for Linux (see flake.nix). On other platforms
      # we still generate the config file below, but install nothing — users
      # there provide outl themselves (e.g. the official installer).
      home.packages = lib.optionals pkgs.stdenv.isLinux (
        [
          cfg.package
        ]
        ++ lib.optional cfg.installDesktop cfg.desktopPackage
      );

      xdg.configFile."outl/config.toml".source = configFile;

      # Both combinations below make `outl serve` exit non-zero, which under
      # Restart=on-failure becomes a crash loop — refuse them at eval time.
      # The { assertion, message } shape is what home-manager's check pass
      # expects; a bare bool here breaks every evaluation of this module.
      assertions = lib.optionals cfg.services.sync.enable [
        {
          assertion = cfg.services.sync.watch || cfg.settings.sync.transport != "file";
          message = "programs.outl.services.sync.watch = false with settings.sync.transport = \"file\" has nothing left to do: outl serve refuses that combination";
        }
        {
          assertion = cfg.services.sync.watch || cfg.services.sync.sync;
          message = "programs.outl.services.sync: watch = false and sync = false is a usage error for outl serve";
        }
      ];

      systemd.user.services.outl-sync =
        lib.mkIf (cfg.services.sync.enable && pkgs.stdenv.hostPlatform.isLinux)
          {
            Unit = {
              Description = "outl background sync service";
              After = [ "network-online.target" ];
              Wants = [ "network-online.target" ];
            };

            Service = {
              Environment = [ "RUST_LOG=${cfg.services.sync.rustLog}" ];
              ExecStart =
                "${cfg.package}/bin/outl serve --workspace ${lib.escapeShellArg cfg.services.sync.workspace}"
                + lib.optionalString (!cfg.services.sync.watch) " --no-watch"
                + lib.optionalString (!cfg.services.sync.sync) " --no-sync";
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
