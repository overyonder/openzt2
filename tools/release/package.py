#!/usr/bin/env python3
"""Build release packages on a configured host."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import plistlib
import re
import shutil
import subprocess
import tomllib
import zipfile

from license_bundle import prepare_release_license_bundle

WORKSPACE = Path(__file__).resolve().parents[2]


def run(*arguments, **options):
    return subprocess.run(arguments, check=True, **options)


def bundle_macos_shared_libraries(binary, destination, original_binary):
    executable_directory = original_binary.parent
    native_libraries = Path(os.environ["OPENZT2_NATIVE_DEPENDENCY_DIR"]) / "lib"
    pending = [(original_binary, binary, ())]
    copied = {}
    while pending:
        original, packaged, inherited_search_paths = pending.pop()

        def expand_loader_path(path):
            if path == "@loader_path" or path.startswith("@loader_path/"):
                return original.parent / path.removeprefix("@loader_path").lstrip("/")
            if path == "@executable_path" or path.startswith("@executable_path/"):
                return executable_directory / path.removeprefix("@executable_path").lstrip("/")
            return Path(path)

        load_commands = subprocess.check_output(["otool", "-l", str(original)], text=True)
        run_paths = re.findall(
            r"cmd LC_RPATH\s+cmdsize \d+\s+path (.+?) \(offset \d+\)", load_commands
        )
        search_paths = tuple(expand_loader_path(path) for path in run_paths) + inherited_search_paths
        imports = subprocess.check_output(["otool", "-L", str(original)], text=True)
        for line in imports.splitlines()[1:]:
            dependency = line.strip().split(" (", 1)[0]
            if dependency.startswith(("/usr/lib/", "/System/")):
                continue
            if dependency.startswith("@rpath/"):
                relative = dependency.removeprefix("@rpath/")
                candidates = [directory / relative for directory in (*search_paths, native_libraries)]
            else:
                candidates = [expand_loader_path(dependency)]
            source = next((path.resolve() for path in candidates if path.is_file()), None)
            if source is None:
                raise RuntimeError(f"Cannot resolve macOS dependency {dependency} from {original}")
            if source == original.resolve():
                continue
            if str(source).startswith(("/usr/lib/", "/System/")):
                continue
            target = destination / source.name
            if source.name in copied and copied[source.name] != source:
                raise RuntimeError(f"Conflicting macOS libraries named {source.name}: {source} and {copied[source.name]}")
            replacement = f"@executable_path/../Frameworks/{source.name}"
            if source.name not in copied:
                shutil.copy2(source, target)
                target.chmod(0o755)
                copied[source.name] = source
                pending.append((source, target, search_paths))
                run("install_name_tool", "-id", replacement, str(target))
            run("install_name_tool", "-change", dependency, replacement, str(packaged))


def bundle_windows_shared_libraries(binary, destination):
    """Follow Windows imports; retain system libraries on their owning OS."""
    pending = [binary]
    copied = set()
    while pending:
        current = pending.pop()
        imports = subprocess.check_output(["objdump", "-p", str(current)], text=True)
        for name in re.findall(r"DLL Name:\s*(\S+)", imports):
            source = shutil.which(name)
            windows_directory = Path(os.environ["SystemRoot"]).resolve()
            if source and not Path(source).resolve().is_relative_to(windows_directory) and name not in copied:
                target = destination / name
                if Path(source).resolve() != target.resolve():
                    shutil.copy2(source, target)
                copied.add(name)
                pending.append(target)


def package(target=None):
    version = tomllib.loads((WORKSPACE / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    system = "Windows" if target else platform.system()
    architecture = "amd64" if target else platform.machine().lower()
    output = WORKSPACE / "build/releases" / f"{system.lower()}-{architecture}"
    staging = output / "staging"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    if system == "Linux" and Path("/etc/NIXOS").exists():
        raise RuntimeError("Portable .deb builds require a native Ubuntu 24.04 release host outside the Nix shell.")
    prefix = Path(os.environ["OPENZT2_NATIVE_DEPENDENCY_DIR"])
    release_target = target or subprocess.check_output(
        ["rustc", "--print", "host-tuple"], text=True).strip()
    licenses = output / "licenses"
    if licenses.exists():
        shutil.rmtree(licenses)
    prepare_release_license_bundle(WORKSPACE, prefix, licenses, release_target)
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=WORKSPACE))
    target_arguments = ["--target", target] if target else []
    run("cargo", "build", "--locked", "--release", "-p", "openzt2_game", "--bin", "openzt2", *target_arguments, cwd=WORKSPACE)
    binary_directory = Path(metadata["target_directory"])
    if target:
        binary_directory /= target
    executable = binary_directory / "release" / ("openzt2.exe" if system == "Windows" else "openzt2")
    name = f"OpenZT2-{version}-{system.lower()}-{architecture}"
    if system == "Windows":
        root = staging / "OpenZT2"
        root.mkdir()
        shutil.copy2(executable, root / executable.name)
        for library in (prefix / "bin").glob("*.dll"):
            shutil.copy2(library, root / library.name)
        if not target:
            for binary in list(root.iterdir()):
                bundle_windows_shared_libraries(binary, root)
        shutil.copy2(WORKSPACE / "README.md", root / "README.txt")
        shutil.copytree(licenses, root / "licenses")
        artifact = output / f"{name}.zip"
        # Nix store files have epoch timestamps; ZIP's earliest date is 1980.
        with zipfile.ZipFile(artifact, "w", compression=zipfile.ZIP_DEFLATED,
                             strict_timestamps=False) as archive:
            for file in sorted(root.rglob("*")):
                if file.is_file():
                    archive.write(file, file.relative_to(staging))
    elif system == "Darwin":
        contents = staging / "OpenZT2.app/Contents"
        binaries = contents / "MacOS"
        libraries = contents / "Frameworks"
        binaries.mkdir(parents=True)
        libraries.mkdir()
        binary = binaries / "openzt2"
        shutil.copy2(executable, binary)
        bundle_version = version.split("-", 1)[0]
        with (contents / "Info.plist").open("wb") as stream:
            plistlib.dump({"CFBundleExecutable": "openzt2", "CFBundleIdentifier": "org.openzt2.game",
                          "CFBundleName": "OpenZT2", "CFBundlePackageType": "APPL",
                          "CFBundleShortVersionString": bundle_version, "CFBundleVersion": bundle_version,
                          "NSHighResolutionCapable": True}, stream)
        bundle_macos_shared_libraries(binary, libraries, executable)
        shutil.copytree(licenses, contents / "Resources/licenses")
        # Ad-hoc signing makes a locally built Apple Silicon app executable; it is not notarization.
        run("codesign", "--force", "--deep", "--sign", "-", str(contents.parent))
        shutil.copy2(WORKSPACE / "README.md", staging / "README.txt")
        (staging / "Applications").symlink_to("/Applications")
        artifact = output / f"{name}.dmg"
        run("hdiutil", "create", "-ov", "-format", "UDZO", "-volname", "OpenZT2", "-srcfolder", str(staging), str(artifact))
    elif system == "Linux":
        root = staging / "package"
        binaries = root / "usr/bin"
        libraries = root / "usr/lib/openzt2"
        control = root / "DEBIAN"
        for directory in (binaries, libraries, control):
            directory.mkdir(parents=True)
        binary = binaries / "openzt2"
        shutil.copy2(executable, binary)
        for pattern in ("libvkd3d-shader.so*", "libmojoshader.so*"):
            for source in (prefix / "lib").glob(pattern):
                shutil.copy2(source, libraries / source.name)
        run("patchelf", "--set-rpath", "$ORIGIN/../lib/openzt2", str(binary))
        for library in libraries.iterdir():
            run("patchelf", "--set-rpath", "$ORIGIN", str(library))
        imports = subprocess.check_output(["ldd", str(binary)], text=True)
        if "not found" in imports or "/nix/store/" in imports:
            raise RuntimeError(f"Package has unresolved or Nix-only imports:\n{imports}")
        deb_arch = {"x86_64": "amd64", "aarch64": "arm64"}[architecture]
        (control / "control").write_text(
            f"Package: openzt2\nVersion: {version.replace('-', '~', 1)}\nArchitecture: {deb_arch}\n"
            "Maintainer: OpenZT2 contributors <noreply@openzt2.org>\n"
            "Depends: libc6 (>= 2.39), libgcc-s1, libstdc++6, libasound2t64, libudev1, libwayland-client0, libxkbcommon0, libvulkan1, libfontconfig1\n"
            "Section: games\nPriority: optional\nDescription: OpenZT2 development build\n"
            " Requires user-supplied Zoo Tycoon 2 archives.\n")
        readme = root / "usr/share/doc/openzt2"
        readme.mkdir(parents=True)
        shutil.copy2(WORKSPACE / "README.md", readme / "README")
        shutil.copytree(licenses, readme / "licenses")
        artifact = output / f"{name}.deb"
        run("dpkg-deb", "--root-owner-group", "--build", str(root), str(artifact))
    else:
        raise RuntimeError(f"Unsupported platform: {system}")
    with artifact.open("rb") as stream:
        checksum = hashlib.file_digest(stream, "sha256").hexdigest()
    (output / "SHA256SUMS").write_text(f"{checksum}  {artifact.name}\n")
    print(artifact)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    package_command = commands.add_parser("package", help="Build and package on a configured release host")
    package_command.add_argument("--target", choices=["x86_64-pc-windows-gnu"], help="Cross-compile Windows using the project Windows shell")
    arguments = parser.parse_args()
    package(arguments.target)


if __name__ == "__main__":
    main()
