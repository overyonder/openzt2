{
  pkgs,
  rustToolchain,
  vkd3dFork,
  mojoshaderFork,
  mojoshaderSource,
  runtimeLibraries,
}: let
  source = pkgs.lib.fileset.toSource {
    root = ../../..;
    fileset = pkgs.lib.fileset.unions [
      ../../../Cargo.toml
      ../../../Cargo.lock
      ../../../.cargo
      ../../../crates
      ../../../vendor/taffy
      ../../../vendor/lunify
      ../../../vendor/bevy_image
      ../../../vendor/bevy_render
      ../../../vendor/bevy_hanabi
      ../../../patches
      ../../../tools/release
      ../../../README.md
      ../../../LICENSE
      ../../../THIRD-PARTY-NOTICES
      ../../../flake.lock
      ../../../.github/workflows/release-builds.yml
    ];
  };
  manifest = builtins.fromTOML (builtins.readFile ../../../Cargo.toml);
  rustPlatform = pkgs.makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };
  vkd3dSource = pkgs.applyPatches {
    name = "openzt2-vkd3d-source";
    src = vkd3dFork.src;
    patches = vkd3dFork.patches;
  };
  nativeNotices = pkgs.runCommand "openzt2-native-notices" {
    nativeBuildInputs = [pkgs.python3];
  } ''
    cp -R ${vkd3dSource} vkd3d
    cp -R ${mojoshaderSource} mojoshader
    chmod -R u+w vkd3d mojoshader
    python3 ${source}/tools/release/native_source_bundle.py \
      --workspace ${source} --vkd3d "$PWD/vkd3d" --mojoshader "$PWD/mojoshader" \
      --destination "$out/share/openzt2-licenses"
  '';
in
  rustPlatform.buildRustPackage {
    pname = "openzt2";
    version = manifest.workspace.package.version;
    src = source;
    cargoLock.lockFile = ../../../Cargo.lock;
    cargoBuildFlags = ["-p" "openzt2_game" "--bin" "openzt2"];
    doCheck = false;
    nativeBuildInputs = [pkgs.pkg-config pkgs.makeWrapper pkgs.python3 pkgs.cargo-about];
    buildInputs = runtimeLibraries ++ [mojoshaderFork vkd3dFork.dev vkd3dFork.lib];
    MOJOSHADER_DIR = mojoshaderSource;
    postInstall = ''
      export PYTHONPATH="$PWD/tools/release"
      python3 - "$PWD" ${nativeNotices} "$out/share/doc/openzt2/licenses" <<'PY'
import sys
from pathlib import Path
from license_bundle import prepare_release_license_bundle
prepare_release_license_bundle(Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]), "x86_64-unknown-linux-gnu")
PY
      install -Dm644 README.md "$out/share/doc/openzt2/README.md"
      install -Dm755 tools/release/linux/openzt2-launch "$out/bin/openzt2-launch"
      install -Dm644 tools/release/linux/org.openzt2.OpenZT2.desktop "$out/share/applications/org.openzt2.OpenZT2.desktop"
      install -Dm644 tools/release/linux/org.openzt2.OpenZT2.svg "$out/share/icons/hicolor/scalable/apps/org.openzt2.OpenZT2.svg"
      install -Dm644 tools/release/linux/org.openzt2.OpenZT2.metainfo.xml "$out/share/metainfo/org.openzt2.OpenZT2.metainfo.xml"
      wrapProgram "$out/bin/openzt2" \
        --prefix LD_LIBRARY_PATH : ${pkgs.lib.makeLibraryPath (runtimeLibraries ++ [mojoshaderFork vkd3dFork.lib])} \
        --set-default FONTCONFIG_FILE ${pkgs.fontconfig.out}/etc/fonts/fonts.conf \
        --run 'export OPENZT2_Z2F_PATH="''${OPENZT2_Z2F_PATH:-''${XDG_DATA_HOME:-$HOME/.local/share}/openzt2/game-files}"'
    '';
    meta = {
      description = "Zoo simulation using user-supplied Zoo Tycoon 2 archives";
      homepage = "https://openzt2.org";
      license = pkgs.lib.licenses.mit;
      mainProgram = "openzt2";
      platforms = ["x86_64-linux"];
    };
  }
