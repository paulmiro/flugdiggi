{
  lib,
  rustPlatform,
  pkg-config,
  makeWrapper,
  libusb1,
  fontconfig,
  freetype,
  libxkbcommon,
  wayland,
  vulkan-loader,
  libx11,
  libxcb,
  libxcursor,
  libxi,
  libxrandr,
}:
let
  workspace = lib.importTOML ./Cargo.toml;
  # Loaded at runtime with dlopen by GPUI.
  runtimeLibs = [
    vulkan-loader
    wayland
    libxkbcommon
    libx11
    libxcb
    libxcursor
    libxi
    libxrandr
  ];
in
rustPlatform.buildRustPackage {
  pname = "flugdiggi-gui";
  inherit (workspace.workspace.package) version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./clippy.toml
      ./core
      ./cli
      ./gui
    ];
  };

  cargoLock.lockFile = ./Cargo.lock;

  cargoBuildFlags = [
    "--package"
    "flugdiggi-gui"
  ];
  cargoTestFlags = [
    "--package"
    "flugdiggi-gui"
  ];

  nativeBuildInputs = [
    pkg-config
    makeWrapper
  ];
  buildInputs = [
    libusb1
    fontconfig
    freetype
    libxkbcommon
    wayland
    libx11
    libxcb
  ];

  postFixup = ''
    wrapProgram $out/bin/flugdiggi-gui --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs}
  '';

  meta = {
    description = "Graphical client for Flydigi Vader 4 controllers";
    license = lib.licenses.unlicense;
    mainProgram = "flugdiggi-gui";
    platforms = lib.platforms.linux;
  };
}
