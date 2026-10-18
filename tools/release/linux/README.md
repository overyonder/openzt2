# Linux packages

The Linux release jobs compile once per architecture on Ubuntu 24.04, then
produce a Debian package, RPM, Arch package, AppImage and Flatpak from each
executable. Every format includes the patched shader libraries, dependency
notices and source bundles. ARM64 packaging is implemented and awaits its first
build and installation checks.

## Compatibility

Native packages and AppImage target x86-64 or ARM64 Linux with glibc 2.39 or
newer. `getconf GNU_LIBC_VERSION` shows the installed version. Flatpak supplies
its own runtime for systems with older glibc versions.

The AppImage bundles the game's shader libraries and uses the host's glibc,
C++ runtime, ALSA, udev, Wayland, XKB, Vulkan loader and Fontconfig libraries.
Install those through your distribution. All formats need a working graphics
driver. Flatpak uses the Freedesktop 25.08 runtime and its graphics extensions.

## Install

Use your distribution's package manager for `.deb`, `.rpm` or `.pkg.tar.zst`:

```sh
sudo apt install ./OpenZT2-*.deb
sudo dnf install ./OpenZT2-*.rpm
sudo pacman -U ./OpenZT2-*.pkg.tar.zst
```

Choose the command for your distribution. For the other formats:

```sh
chmod +x OpenZT2-*.AppImage
./OpenZT2-*.AppImage
# Without FUSE, use --appimage-extract-and-run.

flatpak install --user ./OpenZT2-*.flatpak
flatpak run org.openzt2.OpenZT2
```

Desktop launchers and AppImage look for archives in
`${XDG_DATA_HOME:-$HOME/.local/share}/openzt2/game-files`.
Flatpak uses `~/.var/app/org.openzt2.OpenZT2/data/openzt2/game-files`.
Copy your game archives there. Saves remain in the application's separate
data directory. The sandbox grants no general access to your home directory.

To select another directory, set `OPENZT2_Z2F_PATH` when launching. For
Flatpak, also grant read access to that directory:

```sh
flatpak run --filesystem=/path/to/archives:ro \
  --env=OPENZT2_Z2F_PATH=/path/to/archives org.openzt2.OpenZT2
```

## Build

Use the native Linux build environment in CONTRIBUTING.md. In addition to
its build tools, install `rpm`, `libarchive-tools`, `zstd`, `squashfs-tools`,
`desktop-file-utils`, `librsvg2-bin` and `flatpak`. Install `org.freedesktop.Platform//25.08`
and `org.freedesktop.Sdk//25.08` from Flathub in the user installation.

Run `bash tools/release/linux/install-tools.sh /path/to/packaging-tools x86_64`,
then set `OPENZT2_LINUX_PACKAGE_TOOLS` to that directory and invoke
`package.py package --target linux-x86_64`. ARM64 uses `aarch64` for the tools
and `linux-aarch64` for packaging. AppImage tooling and its runtime have separate
version and checksum pins. The native binary is not rebuilt per format.

The packaging script checks the Flatpak ELF dependencies inside its runtime
and extracts the AppImage to compare its executable with the staged one.
Publication requires all artifacts declared in `../targets.toml` and their checksums.
Missing, unexpected or duplicate entries fail the release job.

To add formats to an existing release without recompiling, verify its published
`.deb` against that release's checksums, extract it with `dpkg-deb -R`, and run:

```sh
mkdir -p output
bash tools/release/linux/package.sh extracted-package output 0.6.0-alpha additional-only
```

This mode leaves the original `.deb` unchanged and creates the four additional
formats. Its checksum file covers those four files only; retain the existing
release checksum entries when combining the manifests for upload.

Alpine APK and Nix installation are covered in the [release packaging guide](../README.md).
