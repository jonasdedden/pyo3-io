"""Puts the freshly built extension on `sys.path` under its module name."""

import shutil
import sys
import sysconfig
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUILT = ROOT / "target" / "debug" / "libpyo3_file_typed_tests.so"


def _install() -> None:
    if not BUILT.exists():
        raise RuntimeError(f"{BUILT} is missing; run `cargo build` in {ROOT} first")
    stage = ROOT / "target" / "pymodule"
    stage.mkdir(parents=True, exist_ok=True)
    suffix = sysconfig.get_config_var("EXT_SUFFIX") or ".so"
    target = stage / f"pyo3_file_typed_tests{suffix}"
    if not target.exists() or target.stat().st_mtime < BUILT.stat().st_mtime:
        shutil.copy2(BUILT, target)
    sys.path.insert(0, str(stage))


_install()


import pytest  # noqa: E402


@pytest.fixture(scope="session")
def stub_source() -> str:
    """The generated stub, which `run-tests.sh` produces before pytest runs."""
    path = ROOT / "typecheck" / "pyo3_file_typed_tests-stubs" / "__init__.pyi"
    if not path.exists():
        pytest.skip(f"{path} is missing; run ./run-tests.sh")
    return path.read_text()
