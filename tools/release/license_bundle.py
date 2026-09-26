"""Assemble license notices and redistributable dependency sources for releases."""

import json
import shutil
import subprocess
import tempfile
from pathlib import Path


def prepare_release_license_bundle(workspace, native_prefix, destination, target):
    native_licenses = native_prefix / "share/openzt2-licenses"
    required = (
        "native-sources.tar.gz",
        "vkd3d/LICENSE",
        "vkd3d/COPYING",
        "vkd3d/AUTHORS",
        "mojoshader/LICENSE.txt",
    )
    for relative in required:
        if not (native_licenses / relative).is_file():
            raise RuntimeError(
                f"Native dependency source or notice is missing: {relative}; rebuild the native prefix"
            )
    shutil.copytree(native_licenses, destination)
    for name in ("LICENSE", "THIRD-PARTY-NOTICES"):
        shutil.copy2(workspace / name, destination / name)
    shutil.copy2(workspace / "crates/game/assets/fonts/ComicNeue-OFL.txt", destination)
    result = subprocess.check_output(
        [
            "cargo",
            "about",
            "generate",
            "--locked",
            "--fail",
            "--manifest-path",
            str(workspace / "crates/game/Cargo.toml"),
            "--target",
            target,
            "--config",
            str(workspace / "tools/release/licenses/about.toml"),
            "--format",
            "json",
        ],
        cwd=workspace,
        text=True,
    )
    write_rust_dependency_notices(json.loads(result), destination)


def write_rust_dependency_notices(report, destination):
    dependency_sources = {}
    with (destination / "RUST-DEPENDENCIES.txt").open("w", encoding="utf-8") as output:
        for license_entry in report["licenses"]:
            output.write(f"{license_entry['name']} ({license_entry['id']})\n")
            for entry in license_entry["used_by"]:
                crate = entry["crate"]
                output.write(f"  {crate['name']} {crate['version']}\n")
                dependency_sources[crate["id"]] = Path(crate["manifest_path"]).parent
            output.write(f"\n{license_entry['text']}\n\n")
    # Preserve supplemental NOTICE and copyright files verbatim, including
    # notices nested in crates that bundle C libraries or data tables.
    for source in dependency_sources.values():
        crate_directory = destination / "crate-notices" / source.name
        for path in source.rglob("*"):
            if path.is_file() and path.name.upper().startswith(("NOTICE", "COPYRIGHT")):
                relative = path.relative_to(source)
                copied = crate_directory / relative
                copied.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, copied)
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary) / "mpl-sources"
        root.mkdir()
        for license_entry in report["licenses"]:
            if license_entry["id"] != "MPL-2.0":
                continue
            for entry in license_entry["used_by"]:
                crate = entry["crate"]
                target = root / f"{crate['name']}-{crate['version']}"
                if not target.exists():
                    shutil.copytree(
                        dependency_sources[crate["id"]],
                        target,
                        ignore=shutil.ignore_patterns(".git", "target", "__pycache__"),
                    )
        if any(root.iterdir()):
            shutil.make_archive(
                str(destination / "mpl-sources"), "gztar", temporary, root.name
            )
