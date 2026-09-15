# services.reelpick — the serving unit, the daily pick, and the timer that fires it.
{ config, lib, pkgs, ... }:
let
  cfg = config.services.reelpick;
  settingsFormat = pkgs.formats.toml { };
  dataDir = "/var/lib/reelpick";
  settingsFile = settingsFormat.generate "reelpick.toml" (
    cfg.settings
    // {
      data_dir = dataDir;
      jellyfin = (cfg.settings.jellyfin or { }) // {
        api_key_file = "/run/credentials/reelpick-pick.service/jellyfin";
      };
      tmdb = (cfg.settings.tmdb or { }) // lib.optionalAttrs (cfg.tmdbApiKeyFile != null) {
        api_key_file = "/run/credentials/reelpick-pick.service/tmdb";
      };
    }
  );
  hardening = {
    User = "reelpick";
    Group = "reelpick";
    ProtectSystem = "strict";
    ProtectHome = true;
    PrivateTmp = true;
    PrivateDevices = true;
    NoNewPrivileges = true;
    RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" ];
    RestrictNamespaces = true;
    RestrictRealtime = true;
    LockPersonality = true;
    MemoryDenyWriteExecute = true;
    SystemCallFilter = [ "@system-service" "~@privileged" ];
    CapabilityBoundingSet = "";
    UMask = "0077";
    StateDirectory = "reelpick";
    WorkingDirectory = dataDir;
  };
in
{
  options.services.reelpick = {
    enable = lib.mkEnableOption "reelpick, one film a day from a Jellyfin library";

    package = lib.mkOption {
      type = lib.types.package;
      description = "The reelpick package to run.";
    };

    settings = lib.mkOption {
      type = settingsFormat.type;
      default = { };
      description = ''
        The configuration file, as Nix. `data_dir` and the two `api_key_file`
        entries are set by this module; everything else is yours. See the
        README for the keys.
      '';
    };

    jellyfinApiKeyFile = lib.mkOption {
      type = lib.types.str;
      example = "/run/secrets/jellyfin-api-key";
      description = ''
        Where the Jellyfin API key comes from, in either form `LoadCredential`
        accepts: an absolute path to a file, or the bare name of a credential
        the service manager itself received (from a container manager, say).
        Loaded into the pick unit only; the serving unit never sees it.
      '';
    };

    tmdbApiKeyFile = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "The TMDB v3 API key, same two forms as `jellyfinApiKeyFile`. Without it, ratings come from Jellyfin.";
    };

    pickTime = lib.mkOption {
      type = lib.types.str;
      default = "05:00";
      description = "When the daily pick runs, in the machine's local time (systemd OnCalendar hour:minute).";
    };
  };

  config = lib.mkIf cfg.enable {
    users.users.reelpick = {
      isSystemUser = true;
      group = "reelpick";
    };
    users.groups.reelpick = { };

    environment.etc."reelpick/reelpick.toml".source = settingsFile;

    systemd.services.reelpick = {
      description = "reelpick — the pages";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      environment.REELPICK_CONFIG = "/etc/reelpick/reelpick.toml";
      restartTriggers = [ settingsFile ];
      serviceConfig = hardening // {
        ExecStart = "${lib.getExe cfg.package} serve";
        Restart = "on-failure";
      };
    };

    systemd.services.reelpick-pick = {
      description = "reelpick — today's pick";
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      environment.REELPICK_CONFIG = "/etc/reelpick/reelpick.toml";
      serviceConfig = hardening // {
        Type = "oneshot";
        ExecStart = "${lib.getExe cfg.package} pick";
        LoadCredential = [ "jellyfin:${cfg.jellyfinApiKeyFile}" ]
          ++ lib.optional (cfg.tmdbApiKeyFile != null) "tmdb:${cfg.tmdbApiKeyFile}";
      };
    };

    systemd.timers.reelpick-pick = {
      description = "reelpick — the daily pick";
      wantedBy = [ "timers.target" ];
      timerConfig = {
        OnCalendar = "*-*-* ${cfg.pickTime}:00";
        Persistent = true;
        Unit = "reelpick-pick.service";
      };
    };
  };
}
