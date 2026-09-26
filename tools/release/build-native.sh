#!/usr/bin/env bash
# Native release hosts supply their platform SDK; source revisions belong to the flake.
set -euo pipefail
workspace=$(cd "$(dirname "$0")/../.." && pwd)
prefix=${OPENZT2_NATIVE_DEPENDENCY_DIR:?Set OPENZT2_NATIVE_DEPENDENCY_DIR}
if command -v cygpath >/dev/null; then prefix=$(cygpath -u "$prefix"); fi
mkdir -p "$prefix" "$prefix/source" "$prefix/include/vkd3d" "$prefix/lib" "$prefix/bin"
prefix=$(cd "$prefix" && pwd)
patch_command=patch
if command -v gpatch >/dev/null; then patch_command=gpatch; fi
for library in vkd3d mojoshader; do
    revision=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["nodes"][sys.argv[2]+"-src"]["locked"]["rev"])' "$workspace/flake.lock" "$library")
    case "$library" in
        vkd3d) remote=https://gitlab.winehq.org/wine/vkd3d.git ;;
        mojoshader) remote=https://github.com/icculus/mojoshader.git ;;
    esac
    source_directory="$prefix/source/$library"
    # A fresh staging directory prevents applying patches twice after a failed build.
    if [[ -e "$source_directory" ]]; then
        echo "Native source staging already exists: $source_directory; use a fresh prefix." >&2
        exit 1
    fi
    if [[ -n ${OPENZT2_NATIVE_SOURCE_DIR:-} ]]; then
        cp -R "$OPENZT2_NATIVE_SOURCE_DIR/$library" "$source_directory"
    else
        git init "$source_directory"
        git -C "$source_directory" fetch --depth 1 "$remote" "$revision"
        git -C "$source_directory" checkout --detach FETCH_HEAD
        for patch in "$workspace/patches/$library/"*.patch; do
            "$patch_command" --batch -d "$source_directory" -p1 < "$patch"
        done
    fi
done
python3 "$workspace/tools/release/native_source_bundle.py" --workspace "$workspace" \
    --vkd3d "$prefix/source/vkd3d" --mojoshader "$prefix/source/mojoshader" \
    --destination "$prefix/share/openzt2-licenses"
cd "$prefix/source/vkd3d"
autoreconf -fi
# Only the shader compiler is built. The configure check for the unused D3D12
# renderer must not require a host Vulkan implementation on the build machine.
case "$(uname -s)" in
    Darwin) export SONAME_LIBVULKAN=libvulkan.dylib ;;
    MINGW*|MSYS*) export SONAME_LIBVULKAN=vulkan-1.dll ;;
    *) export SONAME_LIBVULKAN=libvulkan.so.1 ;;
esac
if command -v x86_64-w64-mingw32-widl >/dev/null; then
    export WIDL=x86_64-w64-mingw32-widl
fi
./configure --enable-shared --disable-static --prefix="$prefix" --disable-tests --disable-demos --without-spirv-tools --without-opengl --without-xcb --without-ncurses
# Direct library targets do not depend on Automake's all-target header rule.
make -j2 -f Makefile -f - openzt2-generated-headers <<'MAKE'
openzt2-generated-headers: $(BUILT_SOURCES)
MAKE
make -j2 libvkd3d-shader.la libvkd3d-shader.pc
# Install only the compiler library, not the separate D3D12 renderer.
./libtool --mode=install install -c libvkd3d-shader.la "$prefix/lib/libvkd3d-shader.la"
cp include/vkd3d_shader.h include/vkd3d_types.h "$prefix/include/vkd3d/"
mkdir -p "$prefix/lib/pkgconfig"
cp libvkd3d-shader.pc "$prefix/lib/pkgconfig/"
cmake -G Ninja -S "$prefix/source/mojoshader" -B "$prefix/mojoshader-build" \
    -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=ON -DCOMPILER_SUPPORT=ON
cmake --build "$prefix/mojoshader-build" --config Release --target mojoshader --parallel 2
cp "$prefix/source/mojoshader/mojoshader.h" "$prefix/source/mojoshader/mojoshader_effects.h" "$prefix/include/"
case "$(uname -s)" in
    Darwin)
        cp "$prefix/mojoshader-build/"*.dylib "$prefix/lib/"
        ;;
    MINGW*|MSYS*)
        cp .libs/*vkd3d-shader*.dll "$prefix/bin/vkd3d-shader.dll"
        cp "$prefix/mojoshader-build/"*mojoshader.dll "$prefix/bin/mojoshader.dll"
        # MSVC consumes COFF import libraries produced from the DLL exports.
        for library in vkd3d-shader mojoshader; do
            (cd "$prefix/bin" && gendef "$library.dll")
            dlltool -d "$prefix/bin/$library.def" -l "$prefix/lib/$library.lib" -D "$library.dll"
        done
        cp -R "${MINGW_PREFIX:-/mingw64}/share/licenses" \
            "$prefix/share/openzt2-licenses/mingw-runtime-notices"
        ;;
    *) cp "$prefix/mojoshader-build/"libmojoshader.so* "$prefix/lib/" ;;
esac
