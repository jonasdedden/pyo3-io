"""The generated type stubs are checked in, so a change to them shows up in review."""

from pathlib import Path

import pytest

EXPECTED = Path(__file__).parent / "expected_stubs.pyi"


def test_matches_the_checked_in_snapshot(stub_source):
    expected = EXPECTED.read_text()
    if stub_source != expected:
        import difflib

        diff = "\n".join(
            difflib.unified_diff(
                expected.splitlines(),
                stub_source.splitlines(),
                "expected_stubs.pyi",
                "generated",
                lineterm="",
            )
        )
        pytest.fail(f"generated stubs differ from the snapshot:\n{diff}")


class TestStubContent:
    def test_only_the_protocols_in_use_are_emitted(self, stub_source):
        """Thirty are linked in; the generator keeps the ones an annotation refers to."""
        emitted = {
            line.split("(")[0].removeprefix("class ")
            for line in stub_source.splitlines()
            if line.startswith("class Supports")
        }
        assert emitted == {
            "SupportsBinaryFileno",
            "SupportsBinaryRead",
            "SupportsBinaryReadSeek",
            "SupportsBinaryReadWrite",
            "SupportsBinaryReadWriteSeekFileno",
            "SupportsBinaryWrite",
            "SupportsTextFileno",
            "SupportsTextRead",
            "SupportsTextReadSeek",
            "SupportsTextReadWrite",
            "SupportsTextWrite",
        }

    def test_no_unused_protocol_leaks_in(self, stub_source):
        for unused in ("SupportsBinarySeek", "SupportsTextSeekFileno", "SupportsTextWriteSeek"):
            assert f"class {unused}(" not in stub_source

    def test_binary_and_text_payloads_are_distinct(self, stub_source):
        assert "def read(self, size: int, /) -> ReadableBuffer: ..." in stub_source
        assert "def read(self, size: int, /) -> str: ..." in stub_source
        assert "def write(self, data: bytes, /) -> int: ..." in stub_source
        assert "def write(self, data: str, /) -> int: ..." in stub_source

    def test_writing_protocols_require_flush(self, stub_source):
        blocks = stub_source.split("class ")
        for block in blocks:
            if "def write(" in block:
                assert "def flush(" in block, f"write without flush in: {block.splitlines()[0]}"

    def test_text_seeking_requires_tell(self, stub_source):
        block = stub_source.split("class SupportsTextReadSeek(Protocol):")[1].split("\nclass ")[0]
        assert "def tell(" in block

    def test_binary_seeking_does_not_require_tell(self, stub_source):
        block = stub_source.split("class SupportsBinaryReadSeek(Protocol):")[1].split("\nclass ")[0]
        assert "def tell(" not in block

    def test_untyped_entry_points_degrade_to_any(self, stub_source):
        """The pyo3-file comparison functions, showing what is lost without the typed wrapper."""
        assert "def legacy_read_all(obj: Any) -> bytes" in stub_source

    def test_every_typed_entry_point_is_annotated(self, stub_source):
        for line in stub_source.splitlines():
            if line.startswith("def ") and not line.startswith("def legacy_"):
                assert ": Supports" in line or ": int" in line or ": bytes" in line or ": str" in line, line
