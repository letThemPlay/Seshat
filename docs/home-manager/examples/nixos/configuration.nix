{ pkgs, ... }:
{
  # Stylix's NixOS module themes the system and, through Home Manager,
  # every user, including Seshat.
  stylix = {
    enable = true;
    base16Scheme = "${pkgs.base16-schemes}/share/themes/gruvbox-light-hard.yaml";
    polarity = "light";
  };

  users.users.alice.isNormalUser = true;
  system.stateVersion = "25.05";
}
