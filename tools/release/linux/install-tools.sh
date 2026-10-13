#!/usr/bin/env bash
# AppImage executables are verified before being run. The embedded runtime is
# pinned separately so appimagetool cannot silently download a newer runtime.
set -euo pipefail
destination=${1:?usage: install-tools.sh DESTINATION}
mkdir -p "$destination"
cd "$destination"
curl --fail --location --retry 3 \
    https://github.com/AppImage/appimagetool/releases/download/1.9.1/appimagetool-x86_64.AppImage \
    -o appimagetool.AppImage
curl --fail --location --retry 3 \
    https://github.com/AppImage/type2-runtime/releases/download/20251108/runtime-x86_64 \
    -o runtime-x86_64
sha256sum --check <<'SUMS'
ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0  appimagetool.AppImage
2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d  runtime-x86_64
SUMS
chmod +x appimagetool.AppImage runtime-x86_64
# Extract once; neither packaging nor its checks require FUSE on CI.
./appimagetool.AppImage --appimage-extract > /dev/null
curl --fail --location --retry 3 \
    https://raw.githubusercontent.com/AppImage/type2-runtime/dd6cebedcbddde9c82f89b011e8e1d40b6e43868/LICENSE \
    -o appimage-runtime-LICENSE

# Include the runtime's build scripts and patched LGPL library sources, so
# recipients can rebuild the embedded runtime with a modified libfuse.
mkdir -p runtime-sources
curl --fail --location --retry 3 \
    https://github.com/AppImage/type2-runtime/archive/dd6cebedcbddde9c82f89b011e8e1d40b6e43868.tar.gz \
    -o runtime-sources/type2-runtime.tar.gz
curl --fail --location --retry 3 \
    https://github.com/libfuse/libfuse/releases/download/fuse-3.15.0/fuse-3.15.0.tar.xz \
    -o runtime-sources/fuse-3.15.0.tar.xz
curl --fail --location --retry 3 \
    https://github.com/vasi/squashfuse/archive/0.5.2.tar.gz \
    -o runtime-sources/squashfuse-0.5.2.tar.gz
sha256sum --check <<'SUMS'
70589cfd5e1cff7ccd6ac91c86c01be340b227285c5e200baa284e401eea2ca0  runtime-sources/fuse-3.15.0.tar.xz
db0238c5981dabbd80ee09ae15387f390091668ca060a7bc38047912491443d3  runtime-sources/squashfuse-0.5.2.tar.gz
SUMS
tar -czf appimage-runtime-sources.tar.gz runtime-sources
mkdir -p runtime-notices
tar -xOf runtime-sources/fuse-3.15.0.tar.xz fuse-3.15.0/LGPL2.txt > runtime-notices/libfuse.txt
tar -xOf runtime-sources/squashfuse-0.5.2.tar.gz squashfuse-0.5.2/LICENSE > runtime-notices/squashfuse.txt
curl --fail --location --retry 3 https://raw.githubusercontent.com/ifduyue/musl/v1.2.5/COPYRIGHT \
    -o runtime-notices/musl.txt
curl --fail --location --retry 3 https://raw.githubusercontent.com/facebook/zstd/v1.5.6/LICENSE \
    -o runtime-notices/zstd.txt
curl --fail --location --retry 3 https://raw.githubusercontent.com/madler/zlib/v1.3.1/LICENSE \
    -o runtime-notices/zlib.txt
curl --fail --location --retry 3 https://raw.githubusercontent.com/microsoft/mimalloc/v2.1.7/LICENSE \
    -o runtime-notices/mimalloc.txt
