"""Artifact discovery follows Cargo output, including configured targets."""

import json
from pathlib import Path
import subprocess
import sys

import pytest

import _extension


def cargo_artifact(built):
    return json.dumps({
        "reason": "compiler-artifact",
        "target": {
            "name": _extension.MODULE,
            "crate_types": ["cdylib"],
            "src_path": str(_extension.ROOT / "src/lib.rs"),
        },
        "filenames": [str(built), str(built.with_suffix(".lib"))],
    })


@pytest.mark.parametrize(
    ("platform", "filename"),
    [
        ("linux", "libpyo3_typed_io_tests.so"),
        ("darwin", "libpyo3_typed_io_tests.dylib"),
        ("win32", "pyo3_typed_io_tests.dll"),
    ],
)
@pytest.mark.parametrize("profile", ["debug", "release"])
def test_artifact_location(monkeypatch, tmp_path, platform, filename, profile):
    target = tmp_path / "custom-target" / "configured-native-triple"
    built = target / profile / filename
    built.parent.mkdir(parents=True)
    built.write_bytes(b"artifact")
    # `_extension.sys` is the `sys` module itself, so patching it patches the same object.
    monkeypatch.setattr(sys, "platform", platform)
    monkeypatch.setattr(
        subprocess,
        "check_output",
        lambda *args, **kwargs: cargo_artifact(built),
    )
    assert _extension.artifact(profile) == built
    monkeypatch.setenv("PYTESTS_PROFILE", profile)
    assert _extension.artifact() == built


def test_missing_artifact_is_an_error(monkeypatch, tmp_path):
    monkeypatch.setattr(
        subprocess,
        "check_output",
        lambda *args, **kwargs: cargo_artifact(tmp_path / "missing.so"),
    )
    with pytest.raises(RuntimeError, match="exactly one built test extension"):
        _extension.artifact("release")


def test_unknown_profile_is_rejected():
    with pytest.raises(ValueError, match="debug or release"):
        _extension.artifact("optimized-ish")


def test_staged_libraries_are_immutable(monkeypatch, tmp_path):
    built = tmp_path / "release/libtest.so"
    built.parent.mkdir()
    monkeypatch.setattr(_extension, "artifact", lambda profile: built)
    monkeypatch.delitem(sys.modules, _extension.MODULE, raising=False)
    monkeypatch.setattr(sys, "path", list(sys.path))
    built.write_bytes(b"original")
    _extension.install("release")
    original = Path(sys.path[0])
    original_file = next(original.iterdir())
    original_stat = original_file.stat()
    _extension.install("release")
    assert original_file.stat().st_mtime_ns == original_stat.st_mtime_ns
    built.write_bytes(b"new build")
    _extension.install("release")
    assert Path(sys.path[0]) != original
    assert original_file.read_bytes() == b"original"
    assert next(Path(sys.path[0]).iterdir()).read_bytes() == b"new build"
