"""Side by side with `pyo3-file`, on the cases the split was meant to fix.

Each test asserts both halves: what the untyped interface does today, and what this crate does
instead. If `pyo3-file` ever fixes one of these, the test says so by failing.
"""

import io
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest

import pyo3_typed_io_tests as ext

LATIN1_TEXT = "éèü" * 8
LATIN1_BYTES = LATIN1_TEXT.encode("latin-1")
GZIP_BYTES = bytes([0x1F, 0x8B, 0x08, 0x00, 0xFF, 0xFE, 0x00, 0x01]) * 4


@pytest.fixture
def latin1_file(tmp_path: Path) -> Path:
    path = tmp_path / "latin1.txt"
    path.write_bytes(LATIN1_BYTES)
    return path


class TestReEncodingCorruption:
    """A text handle re-encoded to UTF-8 does not give back the file's bytes."""

    def test_pyo3_file_hands_over_re_encoded_bytes(self, latin1_file: Path) -> None:
        """A single fixed-size read, so nothing depends on `read_to_end`'s bookkeeping."""
        with open(latin1_file, encoding="latin-1") as handle:
            got = ext.legacy_read_once(handle)
        assert got != LATIN1_BYTES
        assert got == LATIN1_TEXT.encode("utf-8")
        assert len(got) == 2 * len(LATIN1_BYTES)

    def test_pyo3_file_is_exact_for_a_binary_handle(self, latin1_file: Path) -> None:
        """The corruption is specific to the text branch, not to pyo3-file generally."""
        with open(latin1_file, "rb") as handle:
            assert ext.legacy_read_once(handle) == LATIN1_BYTES

    def test_typed_refuses_instead_of_corrupting(self, latin1_file: Path) -> None:
        with open(latin1_file, encoding="latin-1") as handle:
            with pytest.raises(TypeError, match="expected a binary file-like object"):
                # intentional wrong-kind rejection
                ext.binary_read_all(handle)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_typed_binary_gives_the_files_bytes(self, latin1_file: Path) -> None:
        with open(latin1_file, "rb") as handle:
            assert ext.binary_read_all(handle) == LATIN1_BYTES

    def test_typed_text_gives_the_files_characters(self, latin1_file: Path) -> None:
        with open(latin1_file, encoding="latin-1") as handle:
            assert ext.text_read_all(handle) == LATIN1_TEXT


class TestStringIO:
    """`pyo3-file` cannot read a StringIO at all."""

    LENGTHS = range(100)

    @staticmethod
    def _failures(call: Callable[[Any], object]) -> list[int]:
        broken = []
        for n in TestStringIO.LENGTHS:
            try:
                call(io.StringIO("x" * n))
            except Exception:
                broken.append(n)
        return broken

    def test_pyo3_file_fails_for_most_lengths(self) -> None:
        """Its text branch asks for `buf.len() // 4` characters and refuses buffers under 4
        bytes, so whether a read works depends on how the buffer happened to be sized."""
        broken = self._failures(ext.legacy_read_all_text)
        assert len(broken) > 50, f"expected pyo3-file to be broken for most lengths, got {broken}"
        assert 5 in broken

    def test_typed_works_for_every_length(self) -> None:
        assert self._failures(ext.text_read_all) == []

    @pytest.mark.parametrize("text", ["", "a", "ab", "abc", "abcd", "x" * 10_000])
    def test_typed_handles_representative_lengths(self, text: str) -> None:
        assert ext.text_read_all(io.StringIO(text)) == text

    def test_typed_works_for_every_length_of_multibyte_text(self) -> None:
        for n in self.LENGTHS:
            assert ext.text_read_all(io.StringIO("é" * n)) == "é" * n


class TestFilenoPanics:
    """`AsRawFd` cannot fail, so `pyo3-file` panics where Python raises."""

    def test_pyo3_file_panics(self) -> None:
        with pytest.raises(BaseException) as excinfo:
            ext.legacy_fileno(io.BytesIO(b"abc"))
        assert "Panic" in type(excinfo.value).__name__

    def test_typed_returns_an_error(self) -> None:
        with pytest.raises(OSError) as excinfo:
            ext.binary_fileno(io.BytesIO(b"abc"))
        assert "Panic" not in type(excinfo.value).__name__


class TestNonUtf8Write:
    """Writing bytes to a text object means decoding them, which can only fail."""

    def test_pyo3_file_panics(self) -> None:
        with pytest.raises(BaseException) as excinfo:
            ext.legacy_write(io.StringIO(), GZIP_BYTES)
        assert "Panic" in type(excinfo.value).__name__

    def test_typed_refuses_at_extraction(self) -> None:
        with pytest.raises(TypeError, match="expected a binary file-like object"):
            # intentional wrong-kind rejection
            ext.binary_write(io.StringIO(), GZIP_BYTES)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_typed_binary_writes_them_unchanged(self) -> None:
        buffer = io.BytesIO()
        ext.binary_write(buffer, GZIP_BYTES)
        assert buffer.getvalue() == GZIP_BYTES


class TestAnnotationQuality:
    """The point of the exercise: the untyped entry points cannot say anything useful."""

    def test_generated_stub_says_any_for_untyped_entry_points(self, stub_source: str) -> None:
        assert "def legacy_read_all(obj: Any) -> bytes" in stub_source

    def test_generated_stub_is_exact_for_typed_entry_points(self, stub_source: str) -> None:
        assert "def binary_read_all(file: SupportsBinaryRead) -> bytes" in stub_source
        assert "def text_read_all(file: SupportsTextRead) -> str" in stub_source
