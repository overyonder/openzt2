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
    nativeLibraryPatches = directory:
      builtins.map (name: directory + "/${name}")
      (builtins.filter (pkgs.lib.hasSuffix ".patch")
        (builtins.attrNames (builtins.readDir directory)));
    vkd3dFork = pkgs.vkd3d.overrideAttrs (old: {
      version = "2.0-openzt2";
      src = vkd3d-src;
      patches = (old.patches or []) ++ nativeLibraryPatches ./patches/vkd3d;
    });
    mojoshaderSource = pkgs.applyPatches {
      name = "mojoshader-openzt2-source";
      src = mojoshader-src;
      patches = nativeLibraryPatches ./patches/mojoshader;
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
    gamePackage = import ./tools/release/nix/package.nix {
      inherit pkgs rustToolchain vkd3dFork mojoshaderFork mojoshaderSource runtimeLibraries;
    };
  in {
    packages.${system}.default = gamePackage;
    apps.${system}.default = {
      type = "app";
      program = "${gamePackage}/bin/openzt2";
    };
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
