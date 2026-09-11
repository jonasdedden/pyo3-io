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
        assert "def read(self, size: int, /) -> ReadableBuffer | None: ..." in stub_source
        assert "def read(self, size: int, /) -> str | None: ..." in stub_source
        assert "def write(self, data: bytes, /) -> int | None: ..." in stub_source
        assert "def write(self, data: str, /) -> int | None: ..." in stub_source

    def test_protocols_require_exactly_the_named_capabilities(self, stub_source):
        """Structural contracts require methods, not successful runtime behavior."""
        import ast

        for node in ast.parse(stub_source).body:
            if not isinstance(node, ast.ClassDef) or not node.name.startswith("Supports"):
                continue
            expected = {
                method.lower()
                for method in ("Read", "Write", "Seek", "Fileno")
                if method in node.name
            }
            if "Text" in node.name and "Seek" in node.name:
                expected.add("tell")
            methods = [member for member in node.body if isinstance(member, ast.FunctionDef)]
            assert {method.name for method in methods} == expected
            for method in methods:
                if method.name in ("read", "write"):
                    assert isinstance(method.returns, ast.BinOp)
                    assert isinstance(method.returns.op, ast.BitOr)
                    assert isinstance(method.returns.right, ast.Constant)
                    assert method.returns.right.value is None

    def test_writing_protocols_do_not_require_flush(self, stub_source):
        """`flush` is called only when the object has one, so demanding it in the protocol would
        reject minimal writers the runtime accepts. A `Protocol` cannot say "optional"."""
        assert "def flush(" not in stub_source

    def test_text_seeking_requires_tell(self, stub_source):
        block = stub_source.split("class SupportsTextReadSeek(Protocol):")[1].split("\nclass ")[0]
        assert "def tell(" in block

    def test_binary_seeking_does_not_require_tell(self, stub_source):
        block = stub_source.split("class SupportsBinaryReadSeek(Protocol):")[1].split("\nclass ")[0]
        assert "def tell(" not in block

    def test_untyped_entry_points_degrade_to_any(self, stub_source):
        """Neither `pyo3-file` nor `pyo3-filelike` implements `FromPyObject`, so their entry
        points take `Bound<PyAny>` and the stub can say nothing about them."""
        assert "def legacy_read_all(obj: Any) -> bytes" in stub_source
        assert "def filelike_read_all(obj: Any) -> bytes" in stub_source

    def test_every_typed_entry_point_is_annotated(self, stub_source):
        # `legacy_` is pyo3-file and `filelike_` is pyo3-filelike; they are the comparison
        # functions and are expected to be `Any`.
        comparison = ("def legacy_", "def filelike_", "def bench_")
        for line in stub_source.splitlines():
            if line.startswith("def ") and not line.startswith(comparison):
                assert ": Supports" in line or ": int" in line or ": bytes" in line or ": str" in line, line
