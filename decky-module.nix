self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.jovian.decky-loader.flugdiggi;
  loader = config.jovian.decky-loader;
  target = "${loader.stateDir}/plugins/flugdiggi";
in
{
  options.jovian.decky-loader.flugdiggi = {
    enable = lib.mkEnableOption "the flugdiggi plugin for Decky Loader";
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.flugdiggi-decky-plugin;
      defaultText = lib.literalExpression "flugdiggi.packages.\${system}.flugdiggi-decky-plugin";
      description = "The plugin directory to install.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = loader.enable;
        message = "jovian.decky-loader.flugdiggi needs jovian.decky-loader.enable";
      }
    ];

    # The loader chowns its plugin directories, which fails on the Nix store, so the plugin is copied.
    systemd.services.decky-loader.preStart = lib.mkAfter ''
      rm -rf ${target}
      mkdir -p ${loader.stateDir}/plugins
      cp -rL --no-preserve=mode,ownership ${cfg.package} ${target}
      chown -R ${loader.user}: ${target}
    '';
  };
}
