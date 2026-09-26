"""Side by side with `pyo3-filelike`, which already splits binary from text.

Each test asserts what `pyo3-filelike` does today, so it fails if that changes.
"""

import io
import os
from pathlib import Path

import pytest

import pyo3_io_tests as ext

LATIN1_TEXT = "éèü" * 8
LATIN1_BYTES = LATIN1_TEXT.encode("latin-1")


@pytest.fixture
def latin1_file(tmp_path: Path) -> Path:
    path = tmp_path / "latin1.txt"
    path.write_bytes(LATIN1_BYTES)
    return path


class TestWhatItAlreadyGetsRight:
    @pytest.mark.parametrize("n", [0, 1, 5, 37, 1000])
    def test_text_reads_work_at_every_length(self, n: int) -> None:
        assert len(ext.filelike_text_read_all(io.StringIO("x" * n))) == n
        assert len(ext.text_read_all(io.StringIO("x" * n))) == n

    def test_binary_reads_are_exact(self, latin1_file: Path) -> None:
        with open(latin1_file, "rb") as handle:
            assert ext.filelike_read_all(handle) == LATIN1_BYTES
        with open(latin1_file, "rb") as handle:
            assert ext.binary_read_all(handle) == LATIN1_BYTES


class TestTextStillReachesByteConsumers:
    """`PyTextFile` implements `std::io::Read`, so the split stops at the type name."""

    def test_pyo3_filelike_hands_over_re_encoded_bytes(self, latin1_file: Path) -> None:
        with open(latin1_file, encoding="latin-1") as handle:
            got = ext.filelike_text_as_bytes(handle)
        assert got != LATIN1_BYTES
        assert got == LATIN1_TEXT.encode("utf-8")
        assert len(got) == 2 * len(LATIN1_BYTES)


class TestKindDetectionIsAHeuristic:
    """`PyBinaryFile` checks the `mode` attribute and assumes binary when there is none."""

    def test_stringio_is_accepted_then_fails_at_read(self) -> None:
        """`io.StringIO` has no `mode`, so it gets through the check."""
        with pytest.raises(OSError, match="not an instance of 'bytes'"):
            ext.filelike_read_once(io.StringIO("abc"))

    def test_duck_typed_text_reader_is_accepted_then_fails_at_read(self) -> None:
        class DuckText:
            def read(self, size: int = -1, /) -> str:
                return "abc"

        with pytest.raises(OSError, match="not an instance of 'bytes'"):
            ext.filelike_read_once(DuckText())

    def test_typed_refuses_stringio_up_front_with_a_way_out(self) -> None:
        with pytest.raises(TypeError, match=r"expected a binary file-like object.*obj\.buffer"):
            # intentional wrong-kind rejection
            ext.binary_read_all(io.StringIO("abc"))  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_pytextfile_checks_nothing_at_all(self) -> None:
        """Only `PyBinaryFile` has a mode check; a binary object is accepted as text."""
        assert ext.filelike_text_as_bytes(io.BytesIO(b"abc")) == b"abc"

    def test_typed_refuses_it(self) -> None:
        with pytest.raises(TypeError, match="expected a text file-like object"):
            # intentional wrong-kind rejection
            ext.text_read_all(io.BytesIO(b"abc"))  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]


class TestRejectionIsAPanic:
    """`PyBinaryFile::new` is private, so `From`'s `unwrap` is the only way in."""

    def test_pyo3_filelike_panics_on_a_text_mode_file(self, latin1_file: Path) -> None:
        with open(latin1_file, encoding="latin-1") as handle:
            with pytest.raises(BaseException) as excinfo:
                ext.filelike_read_once(handle)
        assert "Panic" in type(excinfo.value).__name__


class TestFilenoPanics:
    """`AsFd` cannot fail, so this is the same panic `pyo3-file` has."""

    @pytest.mark.skipif(os.name != "posix", reason="AsFd is Unix-only; the stand-in returns -1")
    def test_pyo3_filelike_panics(self) -> None:
        with pytest.raises(BaseException) as excinfo:
            ext.filelike_fileno(io.BytesIO(b"abc"))
        assert "Panic" in type(excinfo.value).__name__


class TestNoCapabilityTyping:
    """`PyBinaryFile` is `Read + Write + Seek` whatever the function needed."""

    def test_writing_a_read_only_handle_reaches_runtime(self, latin1_file: Path) -> None:
        with open(latin1_file, "rb") as handle, pytest.raises(Exception) as excinfo:
            ext.filelike_write(handle, b"x")
        assert "write" in str(excinfo.value)
