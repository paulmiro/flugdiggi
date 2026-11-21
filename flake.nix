{
  description = "Linux client for Flydigi Vader 4 controllers";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self, nixpkgs, rust-overlay, ... }:
    let
      forAllSystems = nixpkgs.lib.genAttrs [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        rec {
          flugdiggi-cli = pkgs.callPackage ./package.nix { };
          flugdiggi-gui = pkgs.callPackage ./gui.nix { };
          flugdiggi-decky = flugdiggi-decky-plugin.helper;
          flugdiggi-decky-plugin = pkgs.callPackage ./decky.nix { };
          default = flugdiggi-cli;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ rust-overlay.overlays.default ];
          };
          guiLibs = with pkgs; [
            libusb1
            fontconfig
            freetype
            libxkbcommon
            wayland
            libx11
            libxcb
          ];
          guiRuntimeLibs = with pkgs; [
            vulkan-loader
            wayland
            libxkbcommon
            libx11
            libxcb
            libxcursor
            libxi
            libxrandr
          ];
          rust = pkgs.rust-bin.selectLatestNightlyWith (
            toolchain:
            toolchain.default.override {
              extensions = [
                "rust-src"
                "rust-analyzer"
              ];
            }
          );
        in
        {
          default = pkgs.mkShell {
            packages = [
              rust
              pkgs.pkg-config
            ];
            buildInputs = guiLibs;
            LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath guiRuntimeLibs;
          };
        }
      );
    };
}
