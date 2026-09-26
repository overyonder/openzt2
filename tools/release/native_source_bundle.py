#!/usr/bin/env python3
"""Preserve patched native-library sources before compiling release binaries."""

import argparse
import json
import re
import shutil
import tempfile
from datetime import datetime, timezone
from pathlib import Path


def prepare_native_source_bundle(workspace, vkd3d, mojoshader, destination):
    destination.mkdir(parents=True, exist_ok=True)
    revisions = json.loads((workspace / "flake.lock").read_text())["nodes"]
    with tempfile.TemporaryDirectory() as temporary:
        root = Path(temporary) / "native-sources"
        root.mkdir()
        for name, source in (("vkd3d", vkd3d), ("mojoshader", mojoshader)):
            patches = sorted((workspace / "patches" / name).glob("*.patch"))
            if not patches:
                raise RuntimeError(f"No patch set found for {name}")
            modified_paths = {
                match
                for patch in patches
                for match in re.findall(
                    r"^\+\+\+ b/(.+)$", patch.read_text(), re.MULTILINE
                )
            }
            stamp = datetime.now(timezone.utc).date().isoformat()
            for relative in sorted(modified_paths):
                path = source / relative
                text = path.read_text()
                if "Modified by OpenZT2 contributors" not in text:
                    if path.suffix not in {".c", ".h", ".l", ".y"}:
                        raise RuntimeError(f"Cannot insert a C-style notice in {path}")
                    path.write_text(
                        f"/* Modified by OpenZT2 contributors on {stamp}; see bundled patches. */\n"
                        + text
                    )
            shutil.copytree(
                source,
                root / "source" / name,
                ignore=shutil.ignore_patterns(".git", "mojo-source", "__pycache__"),
            )
            shutil.copytree(
                workspace / "patches" / name, root / "patches" / name
            )
            (root / "source" / name / "OPENZT2-SOURCE.txt").write_text(
                f"Upstream revision: {revisions[name + '-src']['locked']['rev']}\n"
                f"Modified by OpenZT2 contributors; patches are in patches/{name}.\n"
            )
        for filename in ("flake.lock", "LICENSE", "THIRD-PARTY-NOTICES"):
            shutil.copy2(workspace / filename, root / filename)
        (root / "tools/release").mkdir(parents=True)
        for name in (
            "build-native.sh",
            "native_source_bundle.py",
            "windows-dependencies.nix",
        ):
            shutil.copy2(workspace / "tools/release" / name, root / "tools/release" / name)
        (root / "BUILD.txt").write_text(
            "These are the patched sources used for the distributed shader libraries.\n"
            "On a native release host with the tools listed in release-builds.yml, run:\n"
            "  OPENZT2_NATIVE_SOURCE_DIR=$PWD/source \\\n"
            "  OPENZT2_NATIVE_DEPENDENCY_DIR=/absolute/path/to/new-prefix \\\n"
            "  bash tools/release/build-native.sh\n"
            "Use a fresh prefix. No upstream source download is needed.\n"
            "Nix cross-compilation uses tools/release/windows-dependencies.nix.\n"
            "Replace the installed shader shared libraries with ABI-compatible builds.\n"
        )
        workflow = workspace / ".github/workflows/release-builds.yml"
        if workflow.exists():
            shutil.copy2(workflow, root / "release-builds.yml")
        elif (workspace / "release-builds.yml").exists():
            shutil.copy2(workspace / "release-builds.yml", root / "release-builds.yml")
        shutil.make_archive(
            str(destination / "native-sources"), "gztar", temporary, root.name
        )
        for name, source, filenames in (
            ("vkd3d", vkd3d, ("COPYING", "LICENSE", "AUTHORS")),
            ("mojoshader", mojoshader, ("LICENSE.txt",)),
        ):
            notices = destination / name
            notices.mkdir()
            for filename in filenames:
                shutil.copy2(source / filename, notices / filename)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--vkd3d", type=Path, required=True)
    parser.add_argument("--mojoshader", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    arguments = parser.parse_args()
    prepare_native_source_bundle(
        arguments.workspace,
        arguments.vkd3d,
        arguments.mojoshader,
        arguments.destination,
    )


if __name__ == "__main__":
    main()
