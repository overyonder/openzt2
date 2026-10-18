"""Release target metadata shared by workflow generation and packaging."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tomllib

WORKSPACE = Path(__file__).resolve().parents[2]
CONFIGURATION = tomllib.loads((WORKSPACE / "tools/release/targets.toml").read_text())
TARGETS = CONFIGURATION["targets"]
PACKAGING = CONFIGURATION["packaging"]


def workspace_version():
    return tomllib.loads((WORKSPACE / "Cargo.toml").read_text())["workspace"]["package"]["version"]


def release_target(identifier):
    target = TARGETS[identifier]
    return {"id": identifier, **target}


def artifact_names(identifier, version):
    target = TARGETS[identifier]
    names = [f"OpenZT2-{version}-{identifier}.{suffix}" for suffix in target["formats"]]
    if target.get("public_key"):
        names.append(target["public_key"])
    return names


def windows_package_versions(version):
    match = re.fullmatch(r"(\d+)\.(\d+)\.(\d+)(?:-(alpha|beta|rc)(?:\.(\d+))?)?", version)
    if not match:
        raise ValueError(f"Unsupported installer version: {version}")
    major, minor, patch = map(int, match.group(1, 2, 3))
    channel, sequence = match.group(4), int(match.group(5) or 0)
    if major > 255 or minor > 255 or patch > 64 or sequence >= 200:
        raise ValueError("Installer versions require major/minor <= 255, patch <= 64 and prerelease sequence < 200")
    rank = {"alpha": 0, "beta": 200, "rc": 400, None: 999}[channel] + sequence
    return f"{major}.{minor}.{patch * 1000 + rank}", f"{major}.{minor}.{patch}.{rank}"


def write_package_checksums(output, identifier, version):
    lines = []
    for name in artifact_names(identifier, version):
        path = output / name
        if not path.is_file():
            raise FileNotFoundError(path)
        with path.open("rb") as stream:
            digest = hashlib.file_digest(stream, "sha256").hexdigest()
        lines.append(f"{digest}  {name}")
    (output / "SHA256SUMS").write_text("\n".join(lines) + "\n")


def collect_release_packages(root, output, version):
    expected = {name for identifier in TARGETS for name in artifact_names(identifier, version)}
    seen = set()
    checksums = []
    output.mkdir(parents=True, exist_ok=True)
    for manifest in sorted(root.rglob("SHA256SUMS")):
        for line in manifest.read_text().splitlines():
            digest, name = line.split(maxsplit=1)
            name = name.strip()
            if name not in expected or name in seen or Path(name).name != name:
                raise ValueError(f"Unexpected or duplicate release package: {name}")
            package = manifest.parent / name
            with package.open("rb") as stream:
                actual = hashlib.file_digest(stream, "sha256").hexdigest()
            if actual != digest:
                raise ValueError(f"Checksum mismatch: {name}")
            seen.add(name)
            shutil.copy2(package, output / name)
            checksums.append(f"{digest}  {name}")
    if seen != expected:
        raise ValueError(f"Missing release packages: {sorted(expected - seen)}")
    (output / "SHA256SUMS").write_text("\n".join(sorted(checksums)) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    matrix = commands.add_parser("matrix")
    matrix.add_argument("--target", choices=["all", *TARGETS], default="all")
    environment = commands.add_parser("environment")
    environment.add_argument("identifier", choices=TARGETS)
    checksums = commands.add_parser("checksums")
    checksums.add_argument("identifier", choices=TARGETS)
    checksums.add_argument("output", type=Path)
    stage = commands.add_parser("stage")
    stage.add_argument("identifier", choices=TARGETS)
    collect = commands.add_parser("collect")
    collect.add_argument("input", type=Path)
    collect.add_argument("output", type=Path)
    arguments = parser.parse_args()
    version = workspace_version()
    if arguments.command == "matrix":
        identifiers = TARGETS if arguments.target == "all" else [arguments.target]
        matrix = {"include": [release_target(identifier) for identifier in identifiers]}
        rust = re.search(r'rust-bin.stable."([0-9.]+)"', (WORKSPACE / "flake.nix").read_text())[1]
        values = {"matrix": json.dumps(matrix, separators=(",", ":")), "rust": rust,
                  "alpine_image": PACKAGING["alpine_image"], "wix_version": PACKAGING["wix_version"]}
        if os.environ.get("GITHUB_REF_TYPE") == "tag" and os.environ["GITHUB_REF_NAME"] != f"v{version}":
            raise ValueError(f"Release tag must match Cargo.toml: v{version}")
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            for key, value in values.items():
                print(f"{key}={value}", file=output)
    elif arguments.command == "environment":
        target = release_target(arguments.identifier)
        values = {
            "OPENZT2_RELEASE_TARGET": arguments.identifier,
            "OPENZT2_RUST_TARGET": target["rust_target"],
            "OPENZT2_RELEASE_ARCH": target["architecture"],
            "OPENZT2_RELEASE_LIBC": target["libc"],
            "OPENZT2_DIRENV_VERSION": PACKAGING["direnv_version"],
            "MACOSX_DEPLOYMENT_TARGET": PACKAGING["macos_deployment_target"],
            "OPENZT2_FLATPAK_RUNTIME": PACKAGING["flatpak_runtime"],
            "OPENZT2_NATIVE_DEPENDENCY_DIR": str(Path(os.environ["RUNNER_TEMP"]) / "openzt2-native"),
            "KAI_CARGO_TARGET_ROOT": str(Path(os.environ["RUNNER_TEMP"]) / "cargo-targets"),
            "OPENZT2_CARGO_CACHE_TARGET": os.path.relpath(
                Path(os.environ["RUNNER_TEMP"]) / "cargo-targets/openzt2-reimplementation", WORKSPACE
            ).replace("\\", "/"),
        }
        with open(os.environ["GITHUB_ENV"], "a") as output:
            for key, value in values.items():
                print(f"{key}={value}", file=output)
    elif arguments.command == "checksums":
        write_package_checksums(arguments.output, arguments.identifier, version)
    elif arguments.command == "stage":
        output = WORKSPACE / "build/releases" / arguments.identifier
        destination = WORKSPACE / "build/artifacts" / arguments.identifier
        destination.mkdir(parents=True, exist_ok=True)
        for name in [*artifact_names(arguments.identifier, version), "SHA256SUMS"]:
            shutil.copy2(output / name, destination / name)
    else:
        collect_release_packages(arguments.input, arguments.output, version)


if __name__ == "__main__":
    main()
