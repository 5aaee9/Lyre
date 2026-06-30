{ self }:

{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.lyre;
  packages = self.packages.${pkgs.stdenv.hostPlatform.system};
  modelRoot = "${cfg.noiseModels.package}/share/lyre/models";
in
{
  options.services.lyre = {
    enable = lib.mkEnableOption "Lyre Rust API service";

    package = lib.mkOption {
      type = lib.types.package;
      default = packages.lyre-api;
      defaultText = lib.literalExpression "self.packages.\${pkgs.stdenv.hostPlatform.system}.lyre-api";
      description = "Lyre API package to run.";
    };

    noiseModels.package = lib.mkOption {
      type = lib.types.package;
      default = packages.lyre-noise-models;
      defaultText = lib.literalExpression "self.packages.\${pkgs.stdenv.hostPlatform.system}.lyre-noise-models";
      description = "Packaged ONNX denoise models used by the Lyre API service.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "127.0.0.1";
      description = "Host address for the Lyre API listener.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8080;
      description = "TCP port for the Lyre API listener.";
    };

    environment = lib.mkOption {
      type = lib.types.attrsOf lib.types.str;
      default = { };
      description = "Extra environment variables for the Lyre service.";
    };

    envFile = lib.mkOption {
      type = lib.types.nullOr (lib.types.either lib.types.path lib.types.str);
      default = null;
      description = "Optional systemd EnvironmentFile path for secrets or deployment-specific settings.";
    };

    stateDirectory = lib.mkOption {
      type = lib.types.str;
      default = "lyre";
      description = "systemd StateDirectory for Lyre.";
    };

    runtimeDirectory = lib.mkOption {
      type = lib.types.str;
      default = "lyre";
      description = "systemd RuntimeDirectory for Lyre.";
    };

    workingDirectory = lib.mkOption {
      type = lib.types.str;
      default = "/var/lib/lyre";
      description = "Working directory for the Lyre process.";
    };
  };

  config = lib.mkIf cfg.enable {
    systemd.services.lyre = {
      after = [ "network.target" ];
      wantedBy = [ "multi-user.target" ];

      environment = {
        LYRE_HOST = cfg.host;
        LYRE_PORT = toString cfg.port;
        LYRE_DEEPFILTERNET_MODEL_DIR = "${modelRoot}/deepfilternet/onnx";
        LYRE_DPDFNET_MODEL_DIR = "${modelRoot}/dpdfnet/onnx";
      }
      // cfg.environment;

      serviceConfig = {
        Restart = "always";
        DynamicUser = true;
        StateDirectory = cfg.stateDirectory;
        RuntimeDirectory = cfg.runtimeDirectory;
        WorkingDirectory = cfg.workingDirectory;
        StateDirectoryMode = "0700";
        ExecStart = "${cfg.package}/bin/lyre serve";
      }
      // lib.optionalAttrs (cfg.envFile != null) {
        EnvironmentFile = cfg.envFile;
      };
    };
  };
}
