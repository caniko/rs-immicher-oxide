{ config, lib, pkgs, ... }:
let
  cfg = config.services.immich-convert-originals;
  inherit (lib) mkIf mkOption mkEnableOption types literalExpression;
  stateDir = "/var/lib/rs-immicher-oxide";
  optionalArg = name: value: lib.optionals (value != null) [ name value ];
  repeatedArgs = name: values: lib.concatMap (value: [ name value ]) values;
  execArgs =
    [
      "${cfg.package}/bin/rs-immicher-oxide"
      "watch"
      "--immich-url"
      cfg.immichUrl
      "--interval"
      cfg.pollInterval
      "--write-mode"
      cfg.writeMode
      "--state-path"
      cfg.statePath
      "--manifest-path"
      cfg.manifestPath
    ]
    ++ optionalArg "--limit" (if cfg.limit == null then null else toString cfg.limit)
    ++ optionalArg "--created-after" cfg.createdAfter
    ++ optionalArg "--taken-after" cfg.takenAfter
    ++ repeatedArgs "--asset-id" cfg.assetIds;
in
{
  options.services.immich-convert-originals = {
    enable = mkEnableOption "Immich original transcoder (AV1/JXL)";

    package = mkOption {
      type = types.package;
      default = pkgs.rs-immicher-oxide or null;
      defaultText = literalExpression "pkgs.rs-immicher-oxide";
      description = "The rs-immicher-oxide package to use.";
    };

    immichUrl = mkOption {
      type = types.str;
      default = "http://localhost:2283";
      example = "http://localhost:2283";
      description = "Immich server URL (API base).";
    };

    apiKeyFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = ''
        Path to a file containing the Immich API key.
        Use agenix: `config.age.secrets.immich-convert-api-key.path`
      '';
    };

    videoEncoder = mkOption {
      type = types.enum [ "svt-av1-uhq" ];
      default = "svt-av1-uhq";
      description = "Video encoder to use for transcoding.";
    };

    videoCrf = mkOption {
      type = types.int;
      default = 20;
      description = "AV1 CRF value (0-63, lower = better quality).";
    };

    videoPreset = mkOption {
      type = types.int;
      default = 6;
      description = "SVT-AV1 preset (0-13, lower = slower/better).";
    };

    imageEncoder = mkOption {
      type = types.enum [ "jxl-visual-lossless" "jxl-lossless" ];
      default = "jxl-visual-lossless";
      description = "Image encoder to use for transcoding.";
    };

    imageDistance = mkOption {
      type = types.float;
      default = 1.0;
      description = "JPEG XL butteraugli distance (0 = lossless, 1.0 = visually lossless).";
    };

    concurrency = mkOption {
      type = types.int;
      default = 2;
      description = "Number of assets to process concurrently.";
    };

    pollInterval = mkOption {
      type = types.str;
      default = "5min";
      example = "10min";
      description = "Poll interval for new asset discovery (watch mode).";
    };

    writeMode = mkOption {
      type = types.enum [ "dry-run" "upload-only" "trash-original" ];
      default = "dry-run";
      description = ''
        Runtime write behavior. Keep the default dry-run for production
        discovery. Use upload-only for the initial canary so originals remain
        untouched. trash-original moves originals to Immich trash after
        verification; force-delete is not exposed by this module.
      '';
    };

    assetIds = mkOption {
      type = types.listOf types.str;
      default = [];
      description = "Explicit Immich asset IDs to process.";
    };

    limit = mkOption {
      type = types.nullOr types.ints.positive;
      default = null;
      description = "Maximum matching assets to process per media pipeline.";
    };

    createdAfter = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "2026-07-01T00:00:00.000Z";
      description = "Only process assets created after this ISO 8601 timestamp.";
    };

    takenAfter = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "2026-07-01T00:00:00.000Z";
      description = "Only process assets taken after this ISO 8601 timestamp.";
    };

    statePath = mkOption {
      type = types.str;
      default = "${stateDir}/state.jsonl";
      description = "Append-only JSONL state file for verified successes.";
    };

    manifestPath = mkOption {
      type = types.str;
      default = "${stateDir}/manifest.jsonl";
      description = "Append-only JSONL manifest for all run outcomes.";
    };

    renderDevice = mkOption {
      type = types.nullOr types.str;
      default = null;
      example = "/dev/dri/by-path/pci-0000:03:00.0-render";
      description = ''
        VAAPI render device node for GPU-accelerated decode.
        Set to match the Immich ML device for atlas.
      '';
    };

    workDir = mkOption {
      type = types.path;
      default = "/dev/shm/rs-immicher-oxide";
      description = "Working directory for temp files (tmpfs recommended).";
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.apiKeyFile != null;
        message = "services.immich-convert-originals.apiKeyFile must be set";
      }
    ];

    environment.systemPackages = [ cfg.package ];

    users.groups.immich-converter = {};
    users.users.immich-converter = {
      isSystemUser = true;
      group = "immich-converter";
    };

    systemd.services.immich-convert-originals = {
      description = "Immich original transcoder — transpile to AV1/JXL";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      serviceConfig = {
        Type = "simple";
        ExecStart = lib.escapeShellArgs execArgs;
        User = "immich-converter";
        Group = "immich-converter";
        StateDirectory = "rs-immicher-oxide";
        RuntimeDirectory = "rs-immicher-oxide";
        SupplementaryGroups =
          [ "video" "render" ]
          ++ lib.optional (cfg.renderDevice != null) "render";
        AmbientCapabilities = "";
        CapabilityBoundingSet = "";
        PrivateTmp = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        NoNewPrivileges = true;
        Restart = "on-failure";
        RestartSec = "10s";

        EnvironmentFile = cfg.apiKeyFile;
      } // lib.optionalAttrs (cfg.renderDevice != null) {
        DeviceAllow = "${cfg.renderDevice} rw";
      };

      unitConfig.ConditionPathExists = cfg.workDir;
    };

    systemd.tmpfiles.rules = [
      "d ${cfg.workDir} 0750 immich-converter immich-converter -"
    ];
  };
}
