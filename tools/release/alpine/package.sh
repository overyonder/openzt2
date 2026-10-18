#!/usr/bin/env bash
set -euo pipefail
payload=$(realpath "${1:?payload directory}")
output=$(realpath "${2:?output directory}")
version=${3:?release version}
workspace=$(cd "$(dirname "$0")/../../.." && pwd)
architecture=${OPENZT2_RELEASE_ARCH:-x86_64}
identifier=${OPENZT2_RELEASE_TARGET:-linux-x86_64-musl}
name="OpenZT2-$version-$identifier"
alpine_version=$(printf '%s' "$version" | sed -E 's/-(alpha|beta|rc)(\.([0-9]+))?$/_\1\3/')
source_directory="$workspace/tools/release/linux"
install -Dm755 "$source_directory/openzt2-launch" "$payload/usr/bin/openzt2-launch"
install -Dm644 "$source_directory/org.openzt2.OpenZT2.desktop" "$payload/usr/share/applications/org.openzt2.OpenZT2.desktop"
install -Dm644 "$source_directory/org.openzt2.OpenZT2.svg" "$payload/usr/share/icons/hicolor/scalable/apps/org.openzt2.OpenZT2.svg"
install -Dm644 "$source_directory/org.openzt2.OpenZT2.metainfo.xml" "$payload/usr/share/metainfo/org.openzt2.OpenZT2.metainfo.xml"
work="$output/staging/apk"
mkdir -p "$work" "$HOME/.abuild"
key="$HOME/.abuild/openzt2-alpine.rsa"
if [[ -n ${OPENZT2_APK_SIGNING_KEY_BASE64:-} ]]; then
    printf '%s' "$OPENZT2_APK_SIGNING_KEY_BASE64" | base64 -d > "$key"
elif [[ ${GITHUB_REF_TYPE:-} == tag ]]; then
    echo 'Tagged APK releases require OPENZT2_APK_SIGNING_KEY_BASE64.' >&2
    exit 1
else
    openssl genrsa -out "$key" 4096
fi
chmod 600 "$key"
openssl rsa -in "$key" -pubout -out "$output/openzt2-alpine.rsa.pub"
printf 'PACKAGER_PRIVKEY="%s"\n' "$key" > "$HOME/.abuild/abuild.conf"
trap 'rm -f "$key"' EXIT
tar -czf "$work/payload.tar.gz" -C "$payload" usr
source_hash=$(sha512sum "$work/payload.tar.gz" | cut -d' ' -f1)
cat > "$work/APKBUILD" <<EOF
pkgname=openzt2
pkgver=$alpine_version
pkgrel=0
pkgdesc="Zoo simulation using user-supplied Zoo Tycoon 2 archives"
url="https://openzt2.org"
arch="$architecture"
license="MIT BSD-3-Clause LGPL-2.1-or-later Zlib"
depends="musl libgcc libstdc++ alsa-lib eudev-libs wayland-libs-client libxkbcommon vulkan-loader fontconfig"
options="!check"
source="payload.tar.gz"
sha512sums="$source_hash  payload.tar.gz"
package() {
    mkdir -p "\$pkgdir"
    cp -a "\$srcdir/usr" "\$pkgdir/"
}
EOF
(cd "$work" && REPODEST="$work/repository" abuild -F -r)
mapfile -t packages < <(find "$work/repository" -type f -name 'openzt2-*.apk')
[[ ${#packages[@]} == 1 ]] || { echo 'Expected one APK' >&2; exit 1; }
cp "${packages[0]}" "$output/$name.apk"
