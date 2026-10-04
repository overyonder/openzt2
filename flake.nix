{
  description = "OpenZT2 development environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    vkd3d-src = {
      url = "git+https://gitlab.winehq.org/wine/vkd3d.git?rev=9a673d0b43acf51940e745ad156b4cd2f3a9799d";
      flake = false;
    };
    mojoshader-src = {
      url = "github:icculus/mojoshader/cf2661bcb55d9a8732e86f79922592d7efdf5383";
      flake = false;
    };
  };

  outputs = {
    nixpkgs,
    rust-overlay,
    vkd3d-src,
    mojoshader-src,
    ...
  }: let
    system = "x86_64-linux";
    pkgs = import nixpkgs {
      inherit system;
      overlays = [(import rust-overlay)];
    };
    rustToolchain = pkgs.rust-bin.stable."1.96.0".default.override {
      extensions = [
        "clippy"
        "rust-analyzer"
        "rust-src"
        "rustfmt"
      ];
    };
    vkd3dFork = pkgs.vkd3d.overrideAttrs (old: {
      version = "2.0-openzt2";
      src = vkd3d-src;
      patches =
        (old.patches or [])
        ++ [
          ./patches/vkd3d/0001-complete-observed-fx2-emission.patch
          ./patches/vkd3d/0002-complete-corpus-fx2-states.patch
          ./patches/vkd3d/0003-allow-duplicate-effect-pass-names.patch
          ./patches/vkd3d/0004-resolve-effect-state-variables.patch
          ./patches/vkd3d/0005-type-fx2-object-state-values.patch
          ./patches/vkd3d/0006-lower-direct-null-state-values.patch
          ./patches/vkd3d/0007-unwrap-fx2-state-swizzles.patch
          ./patches/vkd3d/0008-emit-fx2-state-expressions.patch
          ./patches/vkd3d/0009-separate-fx2-small-object-count.patch
          ./patches/vkd3d/0010-separate-fx2-object-sections.patch
          ./patches/vkd3d/0011-preserve-fx2-state-constant-types.patch
          ./patches/vkd3d/0012-preserve-fx2-state-vector-components.patch
          ./patches/vkd3d/0013-reset-sm123-constant-allocations-between-effect-shaders.patch
          ./patches/vkd3d/0014-emit-matrix-fx2-state-expressions.patch
          ./patches/vkd3d/0015-reject-value-changing-fx2-expression-casts.patch
        ];
    });
    mojoshaderSource = pkgs.applyPatches {
      name = "mojoshader-openzt2-source";
      src = mojoshader-src;
      patches = [
        ./patches/mojoshader/0001-retain-effect-state-index.patch
        ./patches/mojoshader/0002-retain-effect-state-parameter.patch
        ./patches/mojoshader/0003-resolve-effect-object-parameters.patch
        ./patches/mojoshader/0004-evaluate-effect-state-preshaders.patch
        ./patches/mojoshader/0005-accept-upgraded-shader-ctab-profiles.patch
        ./patches/mojoshader/0006-accept-vkd3d-vs2-sampler-declarations.patch
        ./patches/mojoshader/0007-expose-spirv-binary-size.patch
        ./patches/mojoshader/0008-link-implicit-pixel-shader-inputs.patch
        ./patches/mojoshader/0009-copy-complete-effect-state-vectors.patch
        ./patches/mojoshader/0010-retain-sampler-mappings-and-guard-pointcoord-patches.patch
        ./patches/mojoshader/0011-link-vertex-shaders-without-pixel-shaders.patch
        ./patches/mojoshader/0012-use-integer-windows-file-flags.patch
        ./patches/mojoshader/0013-evaluate-preshaders-with-independent-registers.patch
        ./patches/mojoshader/0014-pack-direct-matrix-state-references.patch
        ./patches/mojoshader/0015-bound-preshader-execution-storage.patch
        ./patches/mojoshader/0016-decode-struct-matrix-members.patch
      ];
    };
    mojoshaderFork = pkgs.mojoshader.overrideAttrs (old: {
      version = "0-openzt2";
      src = mojoshaderSource;
      cmakeFlags = (old.cmakeFlags or []) ++ ["-DCOMPILER_SUPPORT=ON"];
    });
    verificationGamescope = pkgs.gamescope.overrideAttrs (old: {
      patches =
        (old.patches or [])
        ++ [
          ./patches/gamescope/0001-release-command-buffers-before-driver-exit.patch
        ];
    });
    runtimeLibraries = with pkgs; [
      alsa-lib
      fontconfig
      libGL
      libxkbcommon
      udev
      vulkan-loader
      wayland
    ];
  in {
    devShells.${system} = {
      windows = pkgs.mkShell {
        packages = [
          (rustToolchain.override {targets = ["x86_64-pc-windows-gnu"];})
          pkgs.python3
          pkgs.pkgsCross.mingwW64.stdenv.cc
          pkgs.pkgsCross.mingwW64.stdenv.cc.bintools
        ];
        OPENZT2_NATIVE_DEPENDENCY_DIR = import ./tools/release/windows-dependencies.nix {
          inherit pkgs vkd3dFork mojoshaderSource;
        };
        CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = "${pkgs.pkgsCross.mingwW64.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
        CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS = "-L native=${pkgs.pkgsCross.mingwW64.windows.pthreads}/lib";
        CC_x86_64_pc_windows_gnu = "${pkgs.pkgsCross.mingwW64.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
        CXX_x86_64_pc_windows_gnu = "${pkgs.pkgsCross.mingwW64.stdenv.cc}/bin/x86_64-w64-mingw32-g++";
      };
      default = pkgs.mkShell {
        packages = [
          rustToolchain
          pkgs.clang
          verificationGamescope
          pkgs.imagemagick
          pkgs.mold
          pkgs.pkg-config
          pkgs.python3
          pkgs.gh
          pkgs.cargo-about
          pkgs.xdotool
        ];
        buildInputs =
          runtimeLibraries
          ++ [
            mojoshaderFork
            vkd3dFork.dev
            vkd3dFork.lib
          ];

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (runtimeLibraries ++ [mojoshaderFork vkd3dFork.lib]);
        MOJOSHADER_DIR = mojoshaderSource;
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS = "-C link-arg=-fuse-ld=mold";
        CC = "${pkgs.clang}/bin/clang";
        CXX = "${pkgs.clang}/bin/clang++";
        shellHook = ''
          export OPENZT2_Z2F_PATH="''${OPENZT2_Z2F_PATH:-$PWD/vendor/original-software}"
          export CC="${pkgs.clang}/bin/clang"
          export CXX="${pkgs.clang}/bin/clang++"
        '';
      };
    };
  };
}
