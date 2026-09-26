{
  pkgs,
  vkd3dFork,
  mojoshaderSource,
}: let
  cross = pkgs.pkgsCross.mingwW64;
  compiler = "${cross.stdenv.cc}/bin/x86_64-w64-mingw32-gcc";
in
  pkgs.stdenv.mkDerivation {
    pname = "openzt2-windows-shader-libraries";
    version = "0.1.0";
    src = vkd3dFork.src;
    patches = vkd3dFork.patches;
    nativeBuildInputs =
      vkd3dFork.nativeBuildInputs
      ++ [
        cross.stdenv.cc
        cross.stdenv.cc.bintools
        pkgs.cmake
        pkgs.ninja
        pkgs.python3
      ];
    dontUseCmakeConfigure = true;
    dontConfigure = true;
    dontFixup = true;
    buildPhase = ''
        runHook preBuild
      cp -R ${mojoshaderSource} mojo-source
      chmod -R u+w mojo-source
      source_workspace="$TMPDIR/openzt2-source-workspace"
      mkdir -p "$source_workspace/tools/release" "$source_workspace/.github/workflows"
      cp ${../../flake.lock} "$source_workspace/flake.lock"
      cp ${../../LICENSE} "$source_workspace/LICENSE"
      cp ${../../THIRD-PARTY-NOTICES} "$source_workspace/THIRD-PARTY-NOTICES"
      cp -R ${../../patches} "$source_workspace/patches"
      cp ${./build-native.sh} "$source_workspace/tools/release/build-native.sh"
      cp ${./native_source_bundle.py} "$source_workspace/tools/release/native_source_bundle.py"
      cp ${./windows-dependencies.nix} "$source_workspace/tools/release/windows-dependencies.nix"
      cp ${../../.github/workflows/release-builds.yml} "$source_workspace/.github/workflows/release-builds.yml"
      python3 ${./native_source_bundle.py} --workspace "$source_workspace" \
        --vkd3d "$PWD" --mojoshader "$PWD/mojo-source" \
        --destination "$out/share/openzt2-licenses"
      export CC=${compiler}
      export LD=${cross.stdenv.cc}/bin/x86_64-w64-mingw32-ld
      export AR=${cross.stdenv.cc.cc}/bin/x86_64-w64-mingw32-gcc-ar
      export NM=${cross.stdenv.cc.cc}/bin/x86_64-w64-mingw32-gcc-nm
      export RANLIB=${cross.stdenv.cc.cc}/bin/x86_64-w64-mingw32-gcc-ranlib
      export STRIP=${cross.stdenv.cc}/bin/x86_64-w64-mingw32-strip
      export OBJDUMP=${cross.stdenv.cc}/bin/x86_64-w64-mingw32-objdump
        export CPPFLAGS="-I${pkgs.vulkan-headers}/include -I${pkgs.spirv-headers}/include"
        export SONAME_LIBVULKAN=vulkan-1.dll
        ./configure --enable-shared --disable-static --host=x86_64-w64-mingw32 --prefix="$out" \
          --disable-tests --disable-demos --without-spirv-tools \
          --without-opengl --without-xcb --without-ncurses
        printf '%s\n' 'openzt2-generated-headers: $(BUILT_SOURCES)' \
          | make -j2 -f Makefile -f - openzt2-generated-headers
        make -j2 libvkd3d-shader.la
        ${pkgs.stdenv.cc}/bin/cc mojo-source/misc/lemon.c -o "$PWD/lemon-host"
        substituteInPlace mojo-source/CMakeLists.txt \
          --replace-fail 'ADD_EXECUTABLE(lemon "misc/lemon.c")' \
          "ADD_EXECUTABLE(lemon IMPORTED)
           SET_TARGET_PROPERTIES(lemon PROPERTIES IMPORTED_LOCATION \"$PWD/lemon-host\")"
        cmake -G Ninja -S mojo-source -B mojo-build \
        -DCMAKE_SYSTEM_NAME=Windows -DCMAKE_C_COMPILER=${compiler} \
        -DCMAKE_CXX_COMPILER=${cross.stdenv.cc}/bin/x86_64-w64-mingw32-g++ \
          -DCMAKE_RC_COMPILER=${cross.stdenv.cc.bintools}/bin/x86_64-w64-mingw32-windres \
          -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DCOMPILER_SUPPORT=ON
        cmake --build mojo-build --target mojoshader --parallel 2
        runHook postBuild
    '';
    installPhase = ''
      mkdir -p "$out/bin" "$out/lib" "$out/include/vkd3d"
      cp include/vkd3d_shader.h include/vkd3d_types.h "$out/include/vkd3d/"
      cp ${mojoshaderSource}/mojoshader{,_effects}.h "$out/include/"
      cp .libs/*vkd3d-shader*.dll mojo-build/*mojoshader.dll "$out/bin/"
      cp .libs/libvkd3d-shader.dll.a mojo-build/libmojoshader.dll.a "$out/lib/"
      cp "$out/lib/libvkd3d-shader.dll.a" "$out/lib/vkd3d-shader.lib"
      cp "$out/lib/libmojoshader.dll.a" "$out/lib/mojoshader.lib"
      ln -s libvkd3d-shader-1.dll "$out/bin/vkd3d-shader.dll"
      ln -s libmojoshader.dll "$out/bin/mojoshader.dll"
      # Package the cross-compiler's Windows runtimes, never the host's ELF libraries.
      find ${pkgs.lib.getLib cross.stdenv.cc.cc} ${cross.windows.mcfgthreads} \
        -type f -iname '*.dll' -exec cp -t "$out/bin" {} +
    '';
  }
