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
mkdir -p "$work"
key_directory=$(mktemp -d /tmp/openzt2-apk-signing.XXXXXX)
key="$key_directory/openzt2-alpine.rsa"
trap 'rm -f -- "$key" "$key.pub" "$key_directory/abuild.conf" "$key_directory/keys/"*.pub; rmdir -- "$key_directory/keys" "$key_directory"' EXIT
mkdir "$key_directory/keys"
cp /etc/apk/keys/*.pub "$key_directory/keys/"
if [[ -n ${OPENZT2_APK_SIGNING_KEY_BASE64:-} ]]; then
    printf '%s' "$OPENZT2_APK_SIGNING_KEY_BASE64" | base64 -d > "$key"
elif [[ ${GITHUB_REF_TYPE:-} == tag ]]; then
    echo 'Tagged APK releases require OPENZT2_APK_SIGNING_KEY_BASE64.' >&2
    exit 1
else
    openssl genrsa -out "$key" 4096
fi
chmod 600 "$key"
openssl rsa -in "$key" -pubout -out "$key.pub"
cp "$key.pub" "$output/openzt2-alpine.rsa.pub"
cp "$key.pub" "$key_directory/keys/"
printf 'PACKAGER_PRIVKEY="%s"\nPACKAGER="OpenZT2 contributors <noreply@openzt2.org>"\nAPK="apk --keys-dir=%s/keys"\n' "$key" "$key_directory" > "$key_directory/abuild.conf"
tar -czf "$work/payload.tar.gz" -C "$payload" usr
source_hash=$(sha512sum "$work/payload.tar.gz" | cut -d' ' -f1)
cat > "$work/APKBUILD" <<EOF
# Maintainer: OpenZT2 contributors <noreply@openzt2.org>
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
(cd "$work" && ABUILD_USERCONF="$key_directory/abuild.conf" \
    SRCDEST="$work/distfiles" REPODEST="$work/repository" abuild -F -r)
mapfile -t packages < <(find "$work/repository" -type f -name 'openzt2-*.apk')
[[ ${#packages[@]} == 1 ]] || { echo 'Expected one APK' >&2; exit 1; }
cp "${packages[0]}" "$output/$name.apk"
apk --keys-dir "$key_directory/keys" verify "$output/$name.apk"
