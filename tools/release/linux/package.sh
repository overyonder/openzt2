#!/usr/bin/env bash
# Repackage the staged native Linux binary; this script never invokes Cargo.
set -euo pipefail
payload=$(realpath "${1:?usage: package.sh PAYLOAD OUTPUT VERSION}")
output=$(realpath "${2:?output directory}")
version=${3:?workspace version}
mode=${4:-all}
[[ $mode == all || $mode == additional-only ]] || exit 1
tools=$(realpath "${OPENZT2_LINUX_PACKAGE_TOOLS:?Run linux/install-tools.sh first}")
source_directory=$(cd "$(dirname "$0")" && pwd)
architecture=${OPENZT2_RELEASE_ARCH:-$(uname -m)}
case "$architecture" in x86_64|aarch64) ;; *) echo "Unsupported Linux architecture: $architecture" >&2; exit 1 ;; esac
identifier=${OPENZT2_RELEASE_TARGET:-linux-$architecture}
runtime=${OPENZT2_FLATPAK_RUNTIME:-25.08}
[[ $version =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9.]+)?$ ]] || exit 1
name="OpenZT2-$version-$identifier"
app_id=org.openzt2.OpenZT2
work="$output/staging/formats"
mkdir -p "$work"
for required in openzt2-launch "$app_id.desktop" "$app_id.svg" "$app_id.metainfo.xml"; do
    test -f "$source_directory/$required"
done
install -Dm755 "$source_directory/openzt2-launch" "$payload/usr/bin/openzt2-launch"
install -Dm644 "$source_directory/$app_id.desktop" "$payload/usr/share/applications/$app_id.desktop"
install -Dm644 "$source_directory/$app_id.svg" "$payload/usr/share/icons/hicolor/scalable/apps/$app_id.svg"
install -Dm644 "$source_directory/$app_id.metainfo.xml" "$payload/usr/share/metainfo/$app_id.metainfo.xml"
desktop-file-validate "$payload/usr/share/applications/$app_id.desktop"
test -s "$payload/usr/share/doc/openzt2/licenses/native-sources.tar.gz"
test -s "$payload/usr/share/doc/openzt2/licenses/RUST-DEPENDENCIES.txt"

# Rebuild the deb after installing the desktop entry shared by all formats.
if [[ $mode == all ]]; then
    dpkg-deb --root-owner-group --build "$payload" "$output/$name.deb"
    dpkg-deb --info "$output/$name.deb"
fi

rpm_root="$work/rpm"
mkdir -p "$rpm_root"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
rpmbuild -bb --define "_topdir $rpm_root" \
    --define "openzt2_version ${version/-/\~}" \
    --define "openzt2_payload $payload" --define "openzt2_arch $architecture" "$source_directory/openzt2.spec"
mapfile -t rpms < <(find "$rpm_root/RPMS" -name '*.rpm' -type f)
[[ ${#rpms[@]} == 1 ]] || { echo 'Expected exactly one RPM' >&2; exit 1; }
cp "${rpms[0]}" "$output/$name.rpm"
rpm -qp --requires "$output/$name.rpm"

arch_root="$work/arch"
mkdir -p "$arch_root"
cp -a "$payload/usr" "$arch_root/"
cat > "$arch_root/.PKGINFO" <<EOF
pkgname = openzt2
pkgbase = openzt2
pkgver = ${version//-/}-1
pkgdesc = Zoo simulation using user-supplied Zoo Tycoon 2 archives
url = https://github.com/overyonder/openzt2
builddate = ${SOURCE_DATE_EPOCH:-$(date +%s)}
packager = OpenZT2 contributors
size = $(du -sb "$arch_root/usr" | cut -f1)
arch = $architecture
license = MIT
license = BSD-3-Clause
license = LGPL-2.1-or-later
license = Zlib
depend = glibc>=2.39
depend = gcc-libs
depend = alsa-lib
depend = systemd-libs
depend = wayland
depend = libxkbcommon
depend = vulkan-icd-loader
depend = fontconfig
xdata = pkgtype=pkg
EOF
bsdtar -C "$arch_root" --uid 0 --gid 0 --format=mtree \
    --options='!all,use-set,type,uid,gid,mode,time,size,sha256,link' -cf - . \
    | gzip -n > "$work/arch.MTREE"
mv "$work/arch.MTREE" "$arch_root/.MTREE"
bsdtar -C "$arch_root" --uid 0 --gid 0 --zstd -cf "$output/$name.pkg.tar.zst" .
bsdtar -xOf "$output/$name.pkg.tar.zst" ./.PKGINFO

appdir="$work/OpenZT2.AppDir"
mkdir -p "$appdir"
cp -a "$payload/usr" "$appdir/"
cp "$source_directory/$app_id.desktop" "$source_directory/$app_id.svg" "$appdir/"
ln -s usr/bin/openzt2-launch "$appdir/AppRun"
ln -s "$app_id.svg" "$appdir/.DirIcon"
cp "$tools/appimage-runtime-LICENSE" "$appdir/usr/share/doc/openzt2/licenses/"
cp "$tools/appimage-runtime-sources.tar.gz" "$appdir/usr/share/doc/openzt2/licenses/"
cp -a "$tools/runtime-notices" "$appdir/usr/share/doc/openzt2/licenses/appimage-runtime"
ARCH="$architecture" "$tools/squashfs-root/AppRun" --runtime-file "$tools/runtime-$architecture" \
    "$appdir" "$output/$name.AppImage"
chmod +x "$output/$name.AppImage"
mkdir -p "$work/appimage-check"
(cd "$work/appimage-check" && "$output/$name.AppImage" --appimage-extract > /dev/null)
cmp "$payload/usr/bin/openzt2" "$work/appimage-check/squashfs-root/usr/bin/openzt2"

flatpak_root="$work/flatpak"
flatpak build-init --arch="$architecture" "$flatpak_root" "$app_id" \
    org.freedesktop.Sdk org.freedesktop.Platform "$runtime"
cp -a "$payload/usr/." "$flatpak_root/files/"
# A raster export works with Flatpak validators that lack an SVG loader.
mkdir -p "$flatpak_root/files/share/icons/hicolor/128x128/apps"
rsvg-convert "$source_directory/$app_id.svg" \
    -o "$flatpak_root/files/share/icons/hicolor/128x128/apps/$app_id.png"
rm "$flatpak_root/files/share/icons/hicolor/scalable/apps/$app_id.svg"
# Check against the actual runtime, not libraries leaked from the build host.
# shellcheck disable=SC2016 # Variables belong to the shell inside the runtime.
flatpak build "$flatpak_root" sh -ec '
    for elf in /app/bin/openzt2 /app/lib/openzt2/*.so*; do
        output=$(ldd "$elf")
        printf "%s\n" "$output"
        if printf "%s\n" "$output" | grep -E "not found|/nix/store/"; then exit 1; fi
    done
'
flatpak build-finish --command=openzt2-launch --socket=wayland --socket=pulseaudio \
    --share=ipc --share=network --device=dri "$flatpak_root"
flatpak build-export --arch="$architecture" "$work/flatpak-repo" "$flatpak_root" stable
flatpak build-bundle --arch="$architecture" \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$work/flatpak-repo" "$output/$name.flatpak" "$app_id" stable

# These formats deliberately share one compiled ELF and one license bundle.
# Missing artifacts abort the job before upload.
cd "$output"
suffixes=(rpm pkg.tar.zst AppImage flatpak)
[[ $mode != all ]] || suffixes=(deb "${suffixes[@]}")
artifacts=()
for suffix in "${suffixes[@]}"; do
    test -s "$name.$suffix"
    artifacts+=("$name.$suffix")
done
sha256sum "${artifacts[@]}" > SHA256SUMS
sha256sum --check SHA256SUMS
