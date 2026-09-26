"""Locate and stage a Cargo-built extension without depending on pytest."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import sysconfig
import tempfile

ROOT = Path(__file__).resolve().parent.parent
MODULE = "pyo3_io_tests"


def artifact(profile: str | None = None) -> Path:
    """Build incrementally and use Cargo's authoritative artifact path, outside any timing."""
    profile = profile or os.environ.get("PYTESTS_PROFILE", "debug")
    if profile not in ("debug", "release"):
        raise ValueError("profile must be debug or release")
    command = [
        "cargo", "build", "--locked", "--message-format=json-render-diagnostics",
        "--manifest-path", str(ROOT / "Cargo.toml"),
    ]
    if profile == "release":
        command.append("--release")
    output = subprocess.check_output(command, text=True)
    candidates: list[Path] = []
    for line in output.splitlines():
        message = json.loads(line)
        target = message.get("target", {})
        if (
            message.get("reason") == "compiler-artifact"
            and target.get("name") == MODULE
            and "cdylib" in target.get("crate_types", [])
            and Path(target["src_path"]).resolve() == (ROOT / "src/lib.rs").resolve()
        ):
            candidates.extend(
                Path(filename)
                for filename in message["filenames"]
                if Path(filename).suffix in (".so", ".dylib", ".dll")
            )
    if len(candidates) != 1 or not candidates[0].is_file():
        raise RuntimeError("Cargo did not report exactly one built test extension")
    return candidates[0]


def install(profile: str | None = None) -> Path:
    """Stage the selected build under Python's extension-module filename."""
    built = artifact(profile)
    data = built.read_bytes()
    staging = built.parent.parent / "pymodule" / built.parent.name
    stage = staging / hashlib.sha256(data).hexdigest()
    suffix = sysconfig.get_config_var("EXT_SUFFIX")
    if not suffix:
        raise RuntimeError("Python did not report an extension-module suffix")
    target = stage / f"{MODULE}{suffix}"
    loaded = sys.modules.get(MODULE)
    if loaded is not None:
        loaded_file = loaded.__file__
        assert loaded_file is not None, "loaded module has no __file__"
        if Path(loaded_file).resolve() != target.resolve():
            raise RuntimeError("cannot load two extension build profiles in one Python process")
        return built
    if not target.is_file():
        staging.mkdir(parents=True, exist_ok=True)
        # Publish a populated directory atomically. Renaming onto an existing nonempty
        # directory fails on both Unix and Windows, so concurrent processes never truncate
        # or replace a library another process may already have mapped.
        with tempfile.TemporaryDirectory(dir=staging, prefix=".staging-") as tmpdir_name:
            tmpdir = Path(tmpdir_name)
            (tmpdir / target.name).write_bytes(data)
            try:
                tmpdir.rename(stage)
            except OSError:
                if not target.is_file():
                    raise
    sys.path.insert(0, str(stage))
    return built


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("debug", "release"), default=None)
    args = parser.parse_args()
    print(artifact(args.profile))
