"""Locate and stage a Cargo-built extension without depending on pytest."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import sysconfig

ROOT = Path(__file__).resolve().parent.parent
MODULE = "pyo3_file_typed_tests"


def artifact(profile: str | None = None) -> Path:
    """Find the native library, honoring Cargo target-directory configuration."""
    profile = profile or os.environ.get("PYTESTS_PROFILE", "debug")
    if profile not in ("debug", "release"):
        raise ValueError("profile must be debug or release")
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo", "metadata", "--no-deps", "--format-version", "1",
                "--manifest-path", str(ROOT / "Cargo.toml"),
            ],
            text=True,
        )
    )
    target = Path(metadata["target_directory"])
    if sys.platform == "win32":
        library = f"{MODULE}.dll"
    elif sys.platform == "darwin":
        library = f"lib{MODULE}.dylib"
    else:
        library = f"lib{MODULE}.so"
    built = target / profile / library
    if not built.is_file():
        flag = " --release" if profile == "release" else ""
        raise RuntimeError(
            f"{built} is missing; run cargo build --manifest-path {ROOT / 'Cargo.toml'}{flag}"
        )
    return built


def install(profile: str | None = None) -> Path:
    """Stage the selected build under Python's extension-module filename."""
    built = artifact(profile)
    stage = built.parent.parent / "pymodule" / built.parent.name
    suffix = sysconfig.get_config_var("EXT_SUFFIX")
    if not suffix:
        raise RuntimeError("Python did not report an extension-module suffix")
    target = stage / f"{MODULE}{suffix}"
    loaded = sys.modules.get(MODULE)
    if loaded is not None:
        if Path(loaded.__file__).resolve() != target.resolve():
            raise RuntimeError("cannot load two extension build profiles in one Python process")
        return built
    stage.mkdir(parents=True, exist_ok=True)
    # Always copy the selected build: timestamps alone can leave an older extension loaded.
    shutil.copy2(built, target)
    sys.path.insert(0, str(stage))
    return built


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), default=None)
    args = parser.parse_args()
    print(artifact(args.profile))
