"""`None` from `read` or `write` means "not ready yet": a retryable `BlockingIOError`."""

import io
import os
from collections.abc import Iterator

import pytest

import pyo3_io_tests as ext
from helpers import PartialWriter


class NoneThenData:
    """Not ready on the first call, ready on the second."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        self.calls = 0

    def read(self, size: int = -1, /) -> bytes | None:
        if size == 0:
            return b""  # a zero-length read never has to wait
        self.calls += 1
        if self.calls == 1:
            return None
        chunk, self.data = self.data[:size], self.data[size:]
        return chunk


class RaisesBlockingIOError:
    """How buffered streams signal the same thing."""

    def read(self, size: int = -1, /) -> bytes:
        if size == 0:
            return b""
        raise BlockingIOError(11, "Resource temporarily unavailable")


class NoneWriter:
    def __init__(self) -> None:
        self.calls = 0

    def write(self, data: bytes, /) -> int | None:
        self.calls += 1
        return None


class TestReadReturningNone:
    def test_surfaces_as_blockingioerror(self) -> None:
        with pytest.raises(
            BlockingIOError, match=r"nothing could be read.*a later call may succeed"
        ):
            ext.binary_read_exactly(NoneThenData(b"hello"), 8)

    def test_a_retry_on_the_same_object_succeeds(self) -> None:
        obj = NoneThenData(b"hello")
        with pytest.raises(BlockingIOError):
            ext.binary_read_exactly(obj, 8)
        assert ext.binary_read_exactly(obj, 8) == b"hello"

    def test_the_same_for_text(self) -> None:
        class NoneThenText:
            def __init__(self) -> None:
                self.calls = 0

            def read(self, size: int = -1, /) -> str | None:
                if size == 0:
                    return ""
                self.calls += 1
                return None if self.calls == 1 else "hello"

        obj = NoneThenText()
        with pytest.raises(BlockingIOError):
            ext.text_read_chars(obj, 8)
        assert ext.text_read_chars(obj, 8) == "hello"

    def test_an_object_raising_blockingioerror_is_treated_identically(self) -> None:
        with pytest.raises(BlockingIOError):
            ext.binary_read_exactly(RaisesBlockingIOError(), 8)

    def test_empty_bytes_is_end_of_stream_not_would_block(self) -> None:
        assert ext.binary_read_exactly(io.BytesIO(b""), 8) == b""
        assert ext.binary_read_all(io.BytesIO(b"")) == b""


class TestWriteReturningNone:
    def test_surfaces_as_blockingioerror(self) -> None:
        with pytest.raises(BlockingIOError, match="nothing could be written right now"):
            ext.binary_write(NoneWriter(), b"abc")

    def test_nothing_is_silently_claimed_to_have_been_written(self) -> None:
        writer = NoneWriter()
        with pytest.raises(BlockingIOError):
            ext.binary_write(writer, b"abc")
        assert writer.calls == 1


@pytest.mark.skipif(
    not hasattr(os, "set_blocking"), reason="Windows supports os.set_blocking from 3.12"
)
class TestRealNonBlockingPipe:
    @pytest.fixture
    def pipe(self) -> Iterator[tuple[io.FileIO, int]]:
        read_fd, write_fd = os.pipe()
        os.set_blocking(read_fd, False)
        with open(read_fd, "rb", buffering=0) as handle:
            yield handle, write_fd
        os.close(write_fd)

    def test_not_ready_then_ready(self, pipe: tuple[io.FileIO, int]) -> None:
        handle, write_fd = pipe
        with pytest.raises(BlockingIOError):
            ext.binary_read_exactly(handle, 16)
        os.write(write_fd, b"hello")
        assert ext.binary_read_exactly(handle, 16) == b"hello"


class TestHowTheNeighboursHandleIt:
    def test_pyo3_file_reports_a_non_retryable_error_on_read(self) -> None:
        with pytest.raises(OSError) as excinfo:
            ext.legacy_read_once(NoneThenData(b"hello"))
        assert not isinstance(excinfo.value, BlockingIOError)

    def test_pyo3_file_reports_a_non_retryable_error_on_write(self) -> None:
        with pytest.raises(OSError) as excinfo:
            ext.legacy_write(NoneWriter(), b"abc")
        assert not isinstance(excinfo.value, BlockingIOError)

    def test_pyo3_filelike_claims_success_on_a_none_write(self) -> None:
        """It discards `write`'s return value, so nothing written looks like everything written."""
        writer = NoneWriter()
        assert ext.filelike_write(writer, b"abc") == 3
        assert writer.calls == 1  # and nothing actually arrived

    def test_pyo3_filelike_loses_data_on_a_partial_write(self) -> None:
        """The same discarded return value."""
        writer = PartialWriter()
        assert ext.filelike_write(writer, b"0123456789") == 10
        assert writer.value == b"012"  # seven bytes silently dropped
