"""Load the selected extension build and optionally require freshly generated stubs."""

import os
from pathlib import Path

from _extension import install

ROOT = Path(__file__).resolve().parent.parent
install()

import pytest  # noqa: E402


@pytest.fixture(scope="session")
def stub_source() -> str:
    """The generated stub, which `run-tests.sh` produces before pytest runs."""
    if os.environ.get("PYTESTS_STUBS") == "0":
        pytest.skip("stub checks not requested; use ./pytests/run-tests.sh --stubs")
    path = ROOT / "typecheck" / "pyo3_typed_io_tests-stubs" / "__init__.pyi"
    if not path.exists():
        if os.environ.get("PYTESTS_STUBS") == "1":
            pytest.fail(f"required generated stubs are missing: {path}")
        pytest.skip(f"{path} is missing; run ./run-tests.sh")
    return path.read_text()
