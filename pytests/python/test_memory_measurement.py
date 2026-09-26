"""Peak RSS uses an explicit platform metric and unit rather than a guessed conversion."""

import importlib.util
import sys
from collections.abc import Callable
from pathlib import Path

import pytest

SPEC = importlib.util.spec_from_file_location(
    "benchmark_memory", Path(__file__).resolve().parents[1] / "memory.py"
)
assert SPEC is not None
assert SPEC.loader is not None
memory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(memory)


def status_file(content: str) -> Callable[[Path], str]:
    """A `Path.read_text` stand-in that returns `content` for any path."""

    def read_text(self: Path) -> str:
        return content

    return read_text


def test_linux_reads_its_own_address_space_high_water_mark(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(Path, "read_text", status_file("Name:\tpython\nVmHWM:\t12345 kB\n"))
    assert memory.peak_rss() == 12345 * 1024


def test_linux_does_not_guess_missing_metrics(monkeypatch: pytest.MonkeyPatch) -> None:
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(Path, "read_text", status_file("Name:\tpython\n"))
    with pytest.raises(RuntimeError, match="unavailable"):
        memory.peak_rss()
