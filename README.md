# OpenZT2

<img width="1477" height="546" alt="OpenZT2" src="https://github.com/user-attachments/assets/fcffdcda-fe14-4df2-8307-62c97eff160e" />

[![Release builds](https://github.com/overyonder/openzt2/actions/workflows/release-builds.yml/badge.svg)](https://github.com/overyonder/openzt2/actions/workflows/release-builds.yml)
[![Open issues](https://img.shields.io/github/issues-raw/overyonder/openzt2?label=open%20issues&color=red)](https://github.com/overyonder/openzt2/issues?q=is%3Aissue%20is%3Aopen)

OpenZT2 is an open-source engine reimplementation of Zoo Tycoon 2 Ultimate
Collection, a game about building and managing zoos and aquariums. It's written
in Rust with Bevy and is in early beta, with gameplay still in development.

## Downloads

Download a package from [GitHub Releases](https://github.com/overyonder/openzt2/releases).
You'll need your own Zoo Tycoon 2 Ultimate Collection game files to play.

✅ marks a built release package. The [packaging guide](tools/release/README.md)
covers installation and signing.

| OS | Architecture | Package | Built | Install with |
| --- | --- | --- | :---: | --- |
| Windows | x86-64 | `.zip` | ✅ | Extract and run |
| Windows | x86-64 | `.msi` | [CI configured][release-builds] | Windows Installer |
| Windows | x86-64 | `.msix` | [CI configured][release-builds] | [App Installer](tools/release/README.md#signing) |
| Windows | ARM64 | `.zip` | [CI configured][release-builds] | Extract and run |
| macOS | ARM64 (Apple Silicon) | `.dmg` | ✅ | Finder |
| macOS | x86-64 (Intel) | `.dmg` | [CI configured][release-builds] | Finder |
| Linux | x86-64 (glibc) | `.deb` | ✅ | APT / dpkg |
| Linux | x86-64 (glibc) | `.rpm` | ✅ | DNF / Zypper / rpm |
| Linux | x86-64 (glibc) | `.pkg.tar.zst` | ✅ | pacman |
| Linux | x86-64 (glibc) | `.AppImage` | ✅ | Run directly |
| Linux | x86-64 (Freedesktop runtime) | `.flatpak` | ✅ | Flatpak |
| Linux | ARM64 (glibc) | `.deb` | ✅ | APT / dpkg |
| Linux | ARM64 (glibc) | `.rpm` | ✅ | DNF / Zypper / rpm |
| Linux | ARM64 (glibc) | `.pkg.tar.zst` | ✅ | pacman |
| Linux | ARM64 (glibc) | `.AppImage` | ✅ | Run directly |
| Linux | ARM64 (Freedesktop runtime) | `.flatpak` | ✅ | Flatpak |
| Alpine Linux | x86-64 (musl) | `.apk` | ✅ | [apk](tools/release/README.md#alpine) |
| NixOS / Linux with Nix | x86-64 | Flake / `.nar.zst` | ✅ | [Nix](tools/release/README.md#nix) |

Listings for winget, Chocolatey, Scoop, Homebrew, AUR, nixpkgs and Flathub can
also be requested in [Discussions][discussions].

MSIX users need to [install the supplied certificate](tools/release/README.md#signing)
before installing the package.

Native Linux packages and AppImage require glibc 2.39 or newer. Flatpak
supplies its own runtime. The [Linux installation guide](tools/release/linux/README.md)
covers dependencies, installation and game-file locations.

[discussions]: https://github.com/overyonder/openzt2/discussions
[release-builds]: .github/workflows/release-builds.yml

## Features

- Mod reloading from the main menu.
- Construction undo and redo, with Ctrl+Y for redo.
- 64-bit builds for Windows, Linux and macOS.
- Controller input for gameplay and menus.
- Rendering the original game's materials through Bevy using patched
  vkd3d-shader and MojoShader libraries.

<img width="1280" height="960" alt="OpenZT2 gameplay" src="https://github.com/user-attachments/assets/b45e89bf-9c02-4df8-b964-4b7089287e4b" />

## Building and contributing

The Nix flake provides an x86-64 Linux development shell and package definition.
[CONTRIBUTING.md](CONTRIBUTING.md) covers building from source on Linux,
Windows and macOS and the project's contribution guidelines.

Bug reports and missing features are tracked in
[Issues](https://github.com/overyonder/openzt2/issues).

## Community

Visit [openzt2.org](https://openzt2.org) for project details, use
[Discussions][discussions] for questions and package requests, or join the
[Restorers Discord](https://discord.gg/bvcd3ZjSe9) for development updates.

## License

OpenZT2 is licensed under the [MIT License](LICENSE). Release packages include
the licences and notices for their dependencies.
