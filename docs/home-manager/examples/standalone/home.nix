{ config, pkgs, ... }:
{
  home.username = "alice";
  home.homeDirectory = "/home/alice";
  home.stateVersion = "25.05";

  stylix = {
    enable = true;
    base16Scheme = "${pkgs.base16-schemes}/share/themes/gruvbox-dark-hard.yaml";
    polarity = "dark";
  };

  programs.seshat = {
    enable = true;
    # Stylix fills in the theme. Your own values win, one by one:
    theme.colors.text_secondary = config.lib.stylix.colors.withHashtag.base04;
  };
}
