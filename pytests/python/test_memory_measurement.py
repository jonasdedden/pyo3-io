"""Peak RSS uses an explicit platform metric and unit rather than a guessed conversion."""

import importlib.util
from pathlib import Path
import sys

import pytest

SPEC = importlib.util.spec_from_file_location(
    "benchmark_memory", Path(__file__).resolve().parents[1] / "memory.py"
)
assert SPEC is not None
assert SPEC.loader is not None
memory = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(memory)


def test_linux_reads_its_own_address_space_high_water_mark(monkeypatch):
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(
        Path, "read_text", lambda self: "Name:\tpython\nVmHWM:\t12345 kB\n"
    )
    assert memory.peak_rss() == 12345 * 1024


def test_linux_does_not_guess_missing_metrics(monkeypatch):
    monkeypatch.setattr(sys, "platform", "linux")
    monkeypatch.setattr(Path, "read_text", lambda self: "Name:\tpython\n")
    with pytest.raises(RuntimeError, match="unavailable"):
        memory.peak_rss()
