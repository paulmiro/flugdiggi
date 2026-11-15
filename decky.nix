{
  lib,
  stdenv,
  rustPlatform,
  pkg-config,
  libusb1,
  nodejs,
  pnpm_12,
  fetchPnpmDeps,
  pnpmConfigHook,
}:
let
  workspace = lib.importTOML ./Cargo.toml;

  helper = rustPlatform.buildRustPackage {
    pname = "flugdiggi-decky";
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
        ./decky/Cargo.toml
        ./decky/src
      ];
    };

    cargoLock.lockFile = ./Cargo.lock;

    cargoBuildFlags = [
      "--package"
      "flugdiggi-decky"
    ];
    cargoTestFlags = [
      "--package"
      "flugdiggi-decky"
    ];

    nativeBuildInputs = [ pkg-config ];
    buildInputs = [ libusb1 ];

    meta.mainProgram = "flugdiggi-decky";
  };
in
stdenv.mkDerivation (finalAttrs: {
  pname = "flugdiggi-decky-plugin";
  inherit (workspace.workspace.package) version;

  src = lib.fileset.toSource {
    root = ./decky/plugin;
    fileset = lib.fileset.unions [
      ./decky/plugin/LICENSE
      ./decky/plugin/main.py
      ./decky/plugin/package.json
      ./decky/plugin/plugin.json
      ./decky/plugin/pnpm-lock.yaml
      ./decky/plugin/rollup.config.js
      ./decky/plugin/tsconfig.json
      ./decky/plugin/src
    ];
  };

  pnpmDeps = fetchPnpmDeps {
    inherit (finalAttrs) pname version src;
    pnpm = pnpm_12;
    fetcherVersion = 4;
    hash = "sha256-/uUXhay9eNaYDn9P7R7sIW1+F7CU41RNB5V+VEARVLs=";
  };

  nativeBuildInputs = [
    nodejs
    pnpm_12
    pnpmConfigHook
  ];

  buildPhase = ''
    runHook preBuild
    pnpm build
    runHook postBuild
  '';

  # The layout the Decky loader expects; the directory name is chosen by the installer.
  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin $out/dist
    cp plugin.json package.json main.py LICENSE $out/
    cp dist/index.js $out/dist/
    cp ${helper}/bin/flugdiggi-decky $out/bin/
    runHook postInstall
  '';

  passthru = { inherit helper; };

  meta = {
    description = "Decky Loader plugin to edit the button mapping of Flydigi Vader 4 controllers";
    license = lib.licenses.unlicense;
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
  };
})
