# Release packaging

`targets.toml` defines the release runners, architectures and package formats.
The workflow, packager, artifact staging and publication checks read that file.
`flake.nix` owns the Rust version and `flake.lock` pins the native shader sources.

## Targets

| Target | Runner | Packages |
| --- | --- | --- |
| `linux-x86_64` | Ubuntu 24.04 | DEB, RPM, Arch, AppImage, Flatpak |
| `linux-aarch64` | Ubuntu 24.04 ARM64 | DEB, RPM, Arch, AppImage, Flatpak |
| `darwin-arm64` | macOS 26 | DMG |
| `darwin-x86_64` | macOS 26 Intel | DMG |
| `windows-amd64` | Windows Server 2022 | ZIP, MSI, MSIX |
| `windows-arm64` | Windows 11 ARM64 | ZIP |
| `linux-x86_64-musl` | Pinned Alpine container | APK and public signing key |
| `nix-x86_64` | Ubuntu 24.04 with Nix | Compressed store closure and package path |

Native Linux packages and AppImage require glibc 2.39 or newer. Flatpak uses
its own runtime. The musl package is built for Alpine. macOS packages declare
macOS 14 as their minimum version and build native libraries with that target.
These new targets await their first builds and installation checks.

## Local packaging

Follow [CONTRIBUTING.md](../../CONTRIBUTING.md) to prepare the native toolchain
and dependencies. From the repository root, select the matching target:

```sh
export OPENZT2_RELEASE_TARGET=linux-x86_64
export OPENZT2_RELEASE_ARCH=x86_64
export OPENZT2_FLATPAK_RUNTIME=25.08
export OPENZT2_LINUX_PACKAGE_TOOLS=/path/to/packaging-tools
bash tools/release/linux/install-tools.sh "$OPENZT2_LINUX_PACKAGE_TOOLS" "$OPENZT2_RELEASE_ARCH"
direnv exec . python3 tools/release/package.py package --target "$OPENZT2_RELEASE_TARGET"
```

On Linux ARM64, use `linux-aarch64` and `aarch64`. On macOS, use
`darwin-arm64` or `darwin-x86_64`. Windows packaging uses PowerShell with the
matching Visual Studio environment and MSYS2 tools on PATH:

```powershell
python tools/release/package.py package --target windows-amd64
```

Windows x86-64 also needs WiX 5.0.2, the Windows SDK's `makeappx.exe` and
`signtool.exe`, and MSYS2's `rsvg-convert`. ARM64 uses CLANGARM64 and
`OPENZT2_OBJDUMP=llvm-objdump`.

Outputs go to `build/releases/<target>/`. All formats for a target share one
compiled executable and licence bundle. `SHA256SUMS` is written after signing.

## Signing

The workflow uses the GitHub `release` environment. Tagged releases use these
secrets and variable:

| Setting | Kind | Value |
| --- | --- | --- |
| `OPENZT2_MSIX_CERTIFICATE_BASE64` | Secret | Base64-encoded PFX containing the signing certificate and private key |
| `OPENZT2_MSIX_CERTIFICATE_PASSWORD` | Secret | PFX password |
| `OPENZT2_MSIX_PUBLISHER` | Variable | Certificate subject matching the MSIX publisher, such as `CN=OpenZT2` |
| `OPENZT2_APK_SIGNING_KEY_BASE64` | Secret | Base64-encoded RSA private key for Alpine packages |

Manual runs can create a temporary MSIX certificate and APK key. Their public
certificate or key accompanies the workflow artifacts for installation testing.
Tagged releases require the configured credentials.

Windows packages include `openzt2-windows.cer`. Install this public certificate
in the local computer's Trusted People store before installing the MSIX.
MSIX installers require Windows 10 build 19041 or newer.

Windows installer versions preserve prerelease ordering. For `alpha.N`,
`beta.N` and `rc.N`, the revision is respectively `N`, `200 + N` and `400 + N`.
A final release uses revision 999. An omitted sequence is zero. MSI uses
`major.minor.(patch * 1000 + revision)` and MSIX uses
`major.minor.patch.revision`. Major and minor are limited to 255, patch to 64
and prerelease sequences to 199. Out-of-range versions fail packaging.

MSI installs the game under the user's Programs directory. The MSIX manifest
disables AppData write virtualization so saves use the game's normal data
directory across installers. Installation, upgrades, save retention and removal
still need to be exercised on Windows.

## Nix

Build or install from a checkout with flakes enabled:

```sh
nix build .#default
nix profile install .#default
nix run .#default
```

The release job also exports the complete package closure as `.nar.zst` and
includes a `.store-path` file identifying its package. On a trusted local Nix
store, import the closure and install that path:

```sh
zstd -dc OpenZT2-*-nix-x86_64.nar.zst | nix-store --import
nix profile install "$(cat OpenZT2-*-nix-x86_64.store-path)"
```

The package includes desktop integration and dependency notices. Its default
game-file directory is `${XDG_DATA_HOME:-$HOME/.local/share}/openzt2/game-files`.
`OPENZT2_Z2F_PATH` selects another location. Host graphics drivers remain part
of the system configuration.

## Alpine

Download the APK and `openzt2-alpine.rsa.pub` from the same release. After
confirming the release source and checksums, install the public key and package:

```sh
sudo install -m644 openzt2-alpine.rsa.pub /etc/apk/keys/openzt2-alpine.rsa.pub
sudo apk add ./OpenZT2-*-linux-x86_64-musl.apk
```

The desktop launcher uses the same game-file directory as the other native
Linux packages. The APK includes the patched shader libraries and licences.

## Publication

Manual workflow runs upload artifacts. Version tags publish after every target
succeeds and the collected files match the target catalogue and checksums.
Unexpected, duplicate and missing packages stop publication. The tag matches
the workspace version with a `v` prefix, and prerelease tags remain prereleases.

