# A NixOS module that runs `outl serve` as a system service: an always-on
# outl peer, the systemd equivalent of the repo's Dockerfile / docker-compose.yml.
#
# It is one more peer, exactly like a laptop or phone — with one difference: it
# never goes to sleep. It JOINs an existing graph; it never hosts the pairing.
# Read docs/self-hosting.md for the pairing direction and why the two state
# directories are separate; this module encodes both of those invariants.
#
#   { nixos, ... } @ args:
#   let flake = <path-to-this-flake>; in
#   { imports = [ flake.nixosModules.default ];
#     services.outl.enable = true;
#     services.outl.workspace = "/var/lib/outl";
#     services.outl.name = "nas";
#   }

flake:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.outl;
  outlBin = "${cfg.package}/bin/outl";

  # The daemon's environment. $HOME locates the iroh identity
  # ($HOME/.outl/identity.key, which IS this device's node id) and
  # $XDG_CONFIG_HOME locates the device store ($XDG_CONFIG_HOME/outl, the actor
  # bindings). Both live under deviceDir, OUTSIDE the workspace — that
  # separation is what keeps ops-<actor>.jsonl per-device.
  #
  # Deliberately no $OUTL_DEVICE_DIR: it means "throwaway actor" and moves the
  # iroh identity with it, so exporting it would rotate the node id and break
  # every pairing. See outl_sync_iroh::default_device_dir.
  serviceEnv = [
    "HOME=${cfg.deviceDir}"
    "XDG_CONFIG_HOME=${cfg.deviceDir}/.config"
    "OUTL_WORKSPACE=${cfg.workspace}"
    "RUST_LOG=${cfg.rustLog}"
  ];

  # Mirrors docker/entrypoint.sh: make sure the device dirs exist, and create a
  # --bare workspace only when serving one that does not exist yet. --bare
  # writes no ops, so a seeded replica never pushes a second templates/journal
  # page into the graph it joins, and running it before or after pairing is
  # both safe (no actor gets keyed to a throwaway workspace id).
  serveWrapper = pkgs.writeShellScriptBin "outl-serve" ''
    set -euo pipefail
    WORKSPACE="$OUTL_WORKSPACE"
    mkdir -p "$HOME/.outl" "$XDG_CONFIG_HOME/outl"
    if [ ! -f "$WORKSPACE/.outl/config.toml" ]; then
      echo "outl-serve: no workspace at $WORKSPACE — creating one with \`outl init --bare\`" >&2
      ${outlBin} init --bare "$WORKSPACE"
    fi
    exec ${outlBin} --workspace "$WORKSPACE" serve ${lib.optionalString (!cfg.watch) "--no-watch "} ${lib.optionalString (!cfg.sync) "--no-sync"}
  '';

  # The one-time join. Consumes a ticket the operator has placed at
  # $HOME/.pair-ticket (0600, inside the device dir), hands it to
  # `outl peer pair --ticket -` on stdin, then deletes it. The ticket never
  # touches argv (so not `ps`), Nix config, or a world-readable unit file.
  # Safe to re-run: a non-empty peers.json means this device already joined.
  pairWrapper = pkgs.writeShellScriptBin "outl-pair" ''
    set -euo pipefail
    WORKSPACE="$OUTL_WORKSPACE"
    TICKET="$HOME/.pair-ticket"
    PEERS="$WORKSPACE/.outl/peers.json"
    mkdir -p "$HOME/.outl" "$XDG_CONFIG_HOME/outl"
    if [ -s "$PEERS" ]; then
      echo "outl-pair: $PEERS already lists peers — nothing to do (remove it to re-join)" >&2
      exit 0
    fi
    [ -f "$TICKET" ] || { echo "outl-pair: no ticket at $TICKET" >&2; exit 1; }
    tr -d '[:space:]' < "$TICKET" | ${outlBin} --workspace "$WORKSPACE" peer pair --ticket - --name ${lib.escapeShellArg cfg.name}
    rm -f "$TICKET"
  '';
in
{
  options.services.outl = {
    enable = lib.mkEnableOption "an always-on outl sync peer (outl serve as a system service)";

    package = lib.mkOption {
      type = lib.types.package;
      default = flake.packages.${pkgs.system}.outl;
      defaultText = lib.literalExpression "the outl flake's package";
      description = "The outl package to run.";
    };

    workspace = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/outl";
      description = "Path to the outl workspace (the notes: pages/, journals/, ops/, .outl/). Replicated to every paired device.";
    };

    deviceDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/outl-device";
      description = "This device's identity: the iroh key (its node id) and the actor bindings. Must live OUTSIDE the workspace; losing it makes the box come back as a new, unpaired device.";
    };

    name = lib.mkOption {
      type = lib.types.str;
      default = config.networking.hostName or "outl";
      description = "Label this device advertises to its peers (shown in their `outl peer list`).";
    };

    watch = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Run the file watcher (reconciles .md written into the workspace from outside into the op log). Disable for a pure sync relay.";
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
      description = "RUST_LOG for the daemon. Use `outl=debug` when diagnosing a pairing that will not connect.";
    };
  };

  config = lib.mkIf cfg.enable {
    users.groups.outl = { };
    users.users.outl = {
      isSystemUser = true;
      group = "outl";
      home = cfg.deviceDir;
      shell = pkgs.bash;
    };

    # Create and own both state directories before first start. tmpfiles runs
    # at local-fs.target, which the services order after.
    systemd.tmpfiles.rules = [
      "d ${cfg.workspace} 0750 outl outl - -"
      "d ${cfg.deviceDir} 0750 outl outl - -"
      # tmpfiles applies the mode/owner to the final path element only; without
      # this line the intermediate .config dir is left root-owned and the
      # unprivileged wrapper cannot create .config/outl inside it.
      "d ${cfg.deviceDir}/.config 0750 outl outl - -"
      "d ${cfg.deviceDir}/.config/outl 0750 outl outl - -"
    ];

    systemd.services.outl-serve = {
      description = "outl always-on sync peer (outl serve)";
      wantedBy = [ "multi-user.target" ];
      after = [ "local-fs.target" "network-online.target" ];
      wants = [ "network-online.target" ];
      serviceConfig = {
        Type = "simple";
        User = "outl";
        Group = "outl";
        Environment = serviceEnv;
        ExecStart = "${serveWrapper}/bin/outl-serve";
        Restart = "on-failure";
        RestartSec = "5s";
        # The daemon releases its endpoint lease on SIGTERM; a lease left held
        # locks every outl process on this device out of an endpoint. Give it
        # the same 30s the compose file's stop_grace_period allows.
        TimeoutStopSec = "30s";
        # Outbound-only: iroh hole-punches to peers and the relay, nothing
        # dials in, so no firewall opening is needed.
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        ReadWritePaths = [
          cfg.workspace
          cfg.deviceDir
        ];
      };
    };

    # Not enabled by anything: the operator runs `systemctl start outl-pair`
    # once, after placing a ticket (see the module doc above and
    # docs/self-hosting.md → Step 4). NixOS enables every defined service by
    # default, so this must be stated explicitly.
    systemd.services.outl-pair = {
      description = "outl one-time graph join (consumes ${cfg.deviceDir}/.pair-ticket)";
      enable = false;
      after = [ "local-fs.target" ];
      wants = [ "local-fs.target" ];
      serviceConfig = {
        Type = "oneshot";
        User = "outl";
        Group = "outl";
        Environment = lib.remove "RUST_LOG=${cfg.rustLog}" serviceEnv;
        ExecStart = "${pairWrapper}/bin/outl-pair";
        NoNewPrivileges = true;
        PrivateTmp = true;
        ProtectHome = true;
        ProtectSystem = "strict";
        ReadWritePaths = [
          cfg.workspace
          cfg.deviceDir
        ];
      };
    };
  };
}
