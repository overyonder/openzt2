#!/usr/bin/env python3
"""Build release packages on a configured host."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tomllib
import zipfile

from license_bundle import prepare_release_license_bundle
from release_targets import PACKAGING, TARGETS, release_target, workspace_version, write_package_checksums
from windows.package import build_windows_installers

WORKSPACE = Path(__file__).resolve().parents[2]


def run(*arguments, **options):
    return subprocess.run(arguments, check=True, **options)


def require_binary_architecture(binary, target):
    if target["platform"] == "darwin":
        run("lipo", str(binary), "-verify_arch", target["architecture"])
        commands = subprocess.check_output(["otool", "-l", str(binary)], text=True)
        minimums = re.findall(r"\bminos (\d+(?:\.\d+)+)", commands)
        minimums += re.findall(r"cmd LC_VERSION_MIN_MACOSX\s+cmdsize \d+\s+version (\d+(?:\.\d+)+)", commands)
        permitted = tuple(map(int, PACKAGING["macos_deployment_target"].split(".")))
        for minimum in minimums:
            if tuple(map(int, minimum.split("."))) > permitted:
                raise RuntimeError(f"{binary} requires macOS {minimum}, beyond the declared deployment target")
    elif target["platform"] == "windows":
        headers = subprocess.check_output(
            [os.environ.get("OPENZT2_OBJDUMP", "objdump"), "-f", str(binary)], text=True).lower()
        names = ("aarch64", "arm64") if target["architecture"] == "arm64" else ("x86-64", "x86_64")
        if not any(name in headers for name in names):
            raise RuntimeError(f"Unexpected binary architecture: {binary}")
    else:
        with binary.open("rb") as stream:
            header = stream.read(20)
        machine = {"x86_64": 62, "aarch64": 183}[target["architecture"]]
        if len(header) != 20 or header[:4] != b"\x7fELF" or header[4:6] != b"\x02\x01" or int.from_bytes(header[18:20], "little") != machine:
            raise RuntimeError(f"Unexpected ELF architecture: {binary}")


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
                raise RuntimeError(f"Can't resolve macOS dependency {dependency} from {original}")
            if source == original.resolve():
                continue
            if str(source).startswith(("/usr/lib/", "/System/")):
                continue
            target = destination / source.name
            if source.name in copied and copied[source.name] != source:
                raise RuntimeError(f"Conflicting macOS libraries named {source.name}: {source} and {copied[source.name]}")
            replacement = f"@executable_path/../Frameworks/{source.name}"
            if source.name not in copied:
                require_binary_architecture(source, release_target(os.environ["OPENZT2_RELEASE_TARGET"]))
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
        imports = subprocess.check_output([os.environ.get("OPENZT2_OBJDUMP", "objdump"), "-p", str(current)], text=True)
        for name in re.findall(r"DLL Name:\s*(\S+)", imports):
            staged = destination / name
            source = str(staged) if staged.is_file() else shutil.which(name, mode=os.F_OK)
            windows_directory = Path(os.environ["SystemRoot"]).resolve()
            if not source and not name.lower().startswith(("api-ms-", "ext-ms-")):
                raise RuntimeError(f"Missing Windows runtime library: {name}")
            if source and not Path(source).resolve().is_relative_to(windows_directory) and name not in copied:
                target = destination / name
                if Path(source).resolve() != target.resolve():
                    shutil.copy2(source, target)
                copied.add(name)
                pending.append(target)


def package(identifier):
    target = release_target(identifier)
    os.environ["OPENZT2_RELEASE_TARGET"] = identifier
    version = workspace_version()
    output = WORKSPACE / "build/releases" / identifier
    staging = output / "staging"
    if staging.exists():
        shutil.rmtree(staging)
    staging.mkdir(parents=True)
    prefix = Path(os.environ["OPENZT2_NATIVE_DEPENDENCY_DIR"])
    licenses = output / "licenses"
    if licenses.exists():
        shutil.rmtree(licenses)
    prepare_release_license_bundle(WORKSPACE, prefix, licenses, target["rust_target"])
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=WORKSPACE))
    run("cargo", "build", "--locked", "--release", "--target", target["rust_target"],
        "-p", "openzt2_game", "--bin", "openzt2", cwd=WORKSPACE)
    binary_directory = Path(metadata["target_directory"]) / target["rust_target"] / "release"
    executable = binary_directory / ("openzt2.exe" if target["platform"] == "windows" else "openzt2")
    require_binary_architecture(executable, target)
    name = f"OpenZT2-{version}-{identifier}"
    if target["platform"] == "windows":
        payload = staging / "OpenZT2"
        payload.mkdir()
        shutil.copy2(executable, payload / executable.name)
        for library in (prefix / "bin").glob("*.dll"):
            shutil.copy2(library, payload / library.name)
        bundle_windows_shared_libraries(payload / executable.name, payload)
        for library in payload.glob("*.dll"):
            require_binary_architecture(library, target)
        shutil.copy2(WORKSPACE / "README.md", payload / "README.txt")
        shutil.copytree(licenses, payload / "licenses")
        with zipfile.ZipFile(output / f"{name}.zip", "w", compression=zipfile.ZIP_DEFLATED,
                             strict_timestamps=False) as archive:
            for file in sorted(payload.rglob("*")):
                if file.is_file():
                    archive.write(file, file.relative_to(staging))
        build_windows_installers(payload, output, name, version, target["formats"], WORKSPACE / "tools/release")
    elif target["platform"] == "darwin":
        contents = staging / "OpenZT2.app/Contents"
        binaries, libraries = contents / "MacOS", contents / "Frameworks"
        binaries.mkdir(parents=True)
        libraries.mkdir()
        binary = binaries / "openzt2"
        shutil.copy2(executable, binary)
        bundle_version = version.split("-", 1)[0]
        with (contents / "Info.plist").open("wb") as stream:
            plistlib.dump({"CFBundleExecutable": "openzt2", "CFBundleIdentifier": "org.openzt2.game",
                          "CFBundleName": "OpenZT2", "CFBundlePackageType": "APPL",
                          "CFBundleShortVersionString": bundle_version, "CFBundleVersion": bundle_version,
                          "LSMinimumSystemVersion": PACKAGING["macos_deployment_target"],
                          "NSHighResolutionCapable": True}, stream)
        bundle_macos_shared_libraries(binary, libraries, executable)
        shutil.copytree(licenses, contents / "Resources/licenses")
        run("codesign", "--force", "--deep", "--sign", "-", str(contents.parent))
        shutil.copy2(WORKSPACE / "README.md", staging / "README.txt")
        (staging / "Applications").symlink_to("/Applications")
        run("hdiutil", "create", "-ov", "-format", "UDZO", "-volname", "OpenZT2",
            "-srcfolder", str(staging), str(output / f"{name}.dmg"))
    elif target["platform"] == "linux":
        payload = staging / "package"
        binaries, libraries = payload / "usr/bin", payload / "usr/lib/openzt2"
        binaries.mkdir(parents=True)
        libraries.mkdir(parents=True)
        binary = binaries / "openzt2"
        shutil.copy2(executable, binary)
        for pattern in ("libvkd3d-shader.so*", "libmojoshader.so*"):
            for source in (prefix / "lib").glob(pattern):
                shutil.copy2(source, libraries / source.name)
        run("patchelf", "--set-rpath", "$ORIGIN/../lib/openzt2", str(binary))
        for library in libraries.iterdir():
            run("patchelf", "--set-rpath", "$ORIGIN", str(library))
            require_binary_architecture(library, target)
        imports = subprocess.check_output(["ldd", str(binary)], text=True)
        if "not found" in imports or "/nix/store/" in imports:
            raise RuntimeError(f"Unresolved native package imports:\n{imports}")
        documentation = payload / "usr/share/doc/openzt2"
        documentation.mkdir(parents=True)
        shutil.copy2(WORKSPACE / "README.md", documentation / "README")
        shutil.copytree(licenses, documentation / "licenses")
        environment = {**os.environ, "OPENZT2_RELEASE_TARGET": identifier,
                       "OPENZT2_RELEASE_ARCH": target["architecture"],
                       "OPENZT2_FLATPAK_RUNTIME": PACKAGING["flatpak_runtime"]}
        if target["libc"] == "musl":
            run("bash", str(WORKSPACE / "tools/release/alpine/package.sh"),
                str(payload), str(output), version, env=environment)
        else:
            control = payload / "DEBIAN"
            control.mkdir()
            deb_arch = {"x86_64": "amd64", "aarch64": "arm64"}[target["architecture"]]
            (control / "control").write_text(
                f"Package: openzt2\nVersion: {version.replace('-', '~', 1)}\nArchitecture: {deb_arch}\n"
                "Maintainer: OpenZT2 contributors <noreply@openzt2.org>\n"
                "Depends: libc6 (>= 2.39), libgcc-s1, libstdc++6, libasound2t64, libudev1, libwayland-client0, libxkbcommon0, libvulkan1, libfontconfig1\n"
                "Section: games\nPriority: optional\nDescription: OpenZT2 development build\n"
                " Requires user-supplied Zoo Tycoon 2 archives.\n")
            run("bash", str(WORKSPACE / "tools/release/linux/package.sh"),
                str(payload), str(output), version, env=environment)
    else:
        raise ValueError(f"Use the Nix package builder for {identifier}")
    write_package_checksums(output, identifier, version)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    package_command = commands.add_parser("package", help="Build release packages for the selected target")
    package_command.add_argument("--target", choices=TARGETS,
                                 default=os.environ.get("OPENZT2_RELEASE_TARGET"), required=not os.environ.get("OPENZT2_RELEASE_TARGET"))
    arguments = parser.parse_args()
    package(arguments.target)


if __name__ == "__main__":
    main()
