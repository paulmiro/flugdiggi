{
  lib,
  rustPlatform,
  pkg-config,
  libusb1,
}:
let
  workspace = lib.importTOML ./Cargo.toml;
in
rustPlatform.buildRustPackage {
  pname = "flugdiggi-cli";
  inherit (workspace.workspace.package) version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./clippy.toml
      ./core
      ./cli
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  cargoBuildFlags = [
    "--package"
    "flugdiggi-cli"
  ];
  cargoTestFlags = [ "--workspace" ];

  nativeBuildInputs = [ pkg-config ];
  buildInputs = [ libusb1 ];

  meta = {
    description = "Command line client for Flydigi Vader 4 controllers";
    license = lib.licenses.unlicense;
    mainProgram = "flugdiggi";
    platforms = lib.platforms.linux ++ lib.platforms.darwin;
  };
}
