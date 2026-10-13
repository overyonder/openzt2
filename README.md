<img width="1477" height="546" alt="openzt2" src="https://github.com/user-attachments/assets/fcffdcda-fe14-4df2-8307-62c97eff160e" />

[![Release builds](https://github.com/overyonder/openzt2/actions/workflows/release-builds.yml/badge.svg)](https://github.com/overyonder/openzt2/actions/workflows/release-builds.yml)
[![Open issues](https://img.shields.io/github/issues-raw/overyonder/openzt2?label=open%20issues&color=red)](https://github.com/overyonder/openzt2/issues?q=is%3Aissue%20is%3Aopen)

---
OpenZT2 is an open-source engine reimplementation of Zoo Tycoon 2 Ultimate Collection, a simulation game which tasks a player with managing zoos and aquariums.
---

## Downloads and packaging

OpenZT2 is an early alpha. Gameplay remains incomplete; the red badge counts
all open issues, including bugs and missing features. The build badge reports
release packaging status.

Download packages from [GitHub Releases](https://github.com/overyonder/openzt2/releases).
You need your own Zoo Tycoon 2 Ultimate Collection game files; they are not included.

✅ means yes. [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] means no; click it to request support.
“Built” means a package has been produced. Repository listings are maintained
separately from downloadable packages.

| OS | Target | Package format | Built | Install with | Project-maintained repository listing |
| --- | --- | --- | :---: | --- | :---: |
| Windows | x86-64 (GNU) | `.zip` | ✅ | Extract and run | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Windows | x86-64 | `.msi` / `.msix` | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] | Windows Installer / App Installer | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Windows | ARM64 | `.zip` | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] | Extract and run | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| macOS | ARM64 (Apple Silicon) | `.dmg` | ✅ | Finder | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| macOS | x86-64 (Intel) | `.dmg` | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] | Finder | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Linux | x86-64 (glibc) | `.deb` | ✅ | APT / dpkg | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Linux | x86-64 (glibc) | `.rpm` | ✅ | DNF / Zypper / rpm | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Linux | x86-64 (glibc) | `.pkg.tar.zst` | ✅ | pacman | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Linux | x86-64 (glibc) | `.AppImage` | ✅ | Run directly | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Linux | x86-64 (Freedesktop runtime) | `.flatpak` | ✅ | Flatpak | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| Linux | ARM64 / x86-64 musl | Native packages | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] | Distribution package manager | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |
| NixOS / Linux with Nix | x86-64 | Nix package | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] | Nix / nixpkgs | [![Need this? Ask in discussions and we can accommodate.][unavailable]][discussions] |

The Alpha 6 RPM, Arch package, AppImage and Flatpak have been built from the
published Linux executable and are awaiting upload to the release. The ZIP,
DMG and DEB are already published. We have not submitted project-maintained
listings to winget, Chocolatey, Scoop, Homebrew, AUR, nixpkgs or Flathub.
The Nix flake currently provides a development shell.

Native Linux packages and AppImage require **glibc 2.39 or newer**. Flatpak
supplies its own runtime. See the [Linux installation guide](tools/release/linux/README.md)
for dependencies, commands and game-file locations, or [CONTRIBUTING.md](CONTRIBUTING.md)
to build from source.

[unavailable]: docs/images/unavailable.svg "Need this? Ask in discussions and we can accommodate."
[discussions]: https://github.com/overyonder/openzt2/discussions

New features which OpenZT2 include (as of alpha):
- Mod reloading (reload mods from the main menu without restarting the game)
- Re-do functionality (CTRL+Y shortcut)
- 64-bit platform support for Windows, Linux, and macOS
- Controller support for XInput devices
- Modern shader pipeline (vkd3d-shader and MojoShader compile the original D3D9 Effects into shaders and evaluated pass state for Bevy’s renderer)
- Better performance improvements powered by the Bevy engine in Rust

<img width="1280" height="960" alt="image" src="https://github.com/user-attachments/assets/b45e89bf-9c02-4df8-b964-4b7089287e4b" />

## Check out more details about the project on: https://openzt2.org

## Join the Restorers Discord for development updates and sneak-peeks at new features: https://discord.gg/bvcd3ZjSe9
