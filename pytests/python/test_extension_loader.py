"""Artifact discovery follows Cargo's target directory and the host library convention."""

import json

import pytest

import _extension


@pytest.mark.parametrize(
    ("platform", "filename"),
    [
        ("linux", "libpyo3_file_typed_tests.so"),
        ("darwin", "libpyo3_file_typed_tests.dylib"),
        ("win32", "pyo3_file_typed_tests.dll"),
    ],
)
@pytest.mark.parametrize("profile", ["debug", "release"])
def test_artifact_location(monkeypatch, tmp_path, platform, filename, profile):
    target = tmp_path / "custom-target"
    built = target / profile / filename
    built.parent.mkdir(parents=True)
    built.write_bytes(b"artifact")
    monkeypatch.setattr(_extension.sys, "platform", platform)
    monkeypatch.setattr(
        _extension.subprocess,
        "check_output",
        lambda *args, **kwargs: json.dumps({"target_directory": str(target)}),
    )
    assert _extension.artifact(profile) == built
    monkeypatch.setenv("PYTESTS_PROFILE", profile)
    assert _extension.artifact() == built


def test_missing_build_has_an_actionable_error(monkeypatch, tmp_path):
    monkeypatch.setattr(
        _extension.subprocess,
        "check_output",
        lambda *args, **kwargs: json.dumps({"target_directory": str(tmp_path)}),
    )
    with pytest.raises(RuntimeError, match=r"cargo build .* --release"):
        _extension.artifact("release")


def test_unknown_profile_is_rejected():
    with pytest.raises(ValueError, match="debug or release"):
        _extension.artifact("optimized-ish")
