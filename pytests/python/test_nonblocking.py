"""What `None` from `read` or `write` means, and why it is an error but a retryable one.

CPython's io protocol uses `None` for "nothing could be done right now", distinct from `b""`
which means end of stream. It is what an unbuffered non-blocking stream returns when no data has
arrived yet, and the same object returns real data on a later call:

    >>> r, w = os.pipe(); os.set_blocking(r, False)
    >>> f = open(r, "rb", buffering=0)
    >>> f.read(10)          # nothing yet
    >>> os.write(w, b"hello")
    >>> f.read(10)
    b'hello'

So it maps to `io::ErrorKind::WouldBlock`, which is Rust's name for exactly this, and comes back
out to Python as `BlockingIOError` — the same exception a buffered stream raises on its own for
the same condition, and the one `pyo3` already maps to `WouldBlock` in the other direction.
"""

import io
import os

import pytest

import pyo3_typed_io_tests as ext


class NoneThenData:
    """Not ready on the first call, ready on the second. What a real socket does."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        self.calls = 0

    def read(self, size: int = -1, /):
        if size == 0:
            return b""  # a zero-length read never has to wait
        self.calls += 1
        if self.calls == 1:
            return None
        chunk, self.data = self.data[:size], self.data[size:]
        return chunk


class RaisesBlockingIOError:
    """The other way Python signals the same thing, used by buffered streams."""

    def read(self, size: int = -1, /):
        if size == 0:
            return b""
        raise BlockingIOError(11, "Resource temporarily unavailable")


class NoneWriter:
    def __init__(self) -> None:
        self.calls = 0

    def write(self, data: bytes, /):
        self.calls += 1
        return None


class TestReadReturningNone:
    def test_surfaces_as_blockingioerror(self):
        with pytest.raises(BlockingIOError):
            ext.binary_read_exactly(NoneThenData(b"hello"), 8)

    def test_the_message_explains_that_a_retry_may_work(self):
        with pytest.raises(BlockingIOError, match="a later call may succeed"):
            ext.binary_read_exactly(NoneThenData(b"hello"), 8)

    def test_a_retry_on_the_same_object_succeeds(self):
        """The whole reason it must not be a fatal error."""
        obj = NoneThenData(b"hello")
        with pytest.raises(BlockingIOError):
            ext.binary_read_exactly(obj, 8)
        assert ext.binary_read_exactly(obj, 8) == b"hello"

    def test_the_same_for_text(self):
        class NoneThenText:
            def __init__(self):
                self.calls = 0

            def read(self, size: int = -1, /):
                if size == 0:
                    return ""
                self.calls += 1
                return None if self.calls == 1 else "hello"

        obj = NoneThenText()
        with pytest.raises(BlockingIOError):
            ext.text_read_chars(obj, 8)
        assert ext.text_read_chars(obj, 8) == "hello"

    def test_an_object_raising_blockingioerror_is_treated_identically(self):
        """Both signalling styles have to land on the same thing, or a caller cannot handle one."""
        with pytest.raises(BlockingIOError):
            ext.binary_read_exactly(RaisesBlockingIOError(), 8)

    def test_empty_bytes_is_end_of_stream_not_would_block(self):
        """`b""` and `None` mean different things, and are treated differently."""
        assert ext.binary_read_exactly(io.BytesIO(b""), 8) == b""
        assert ext.binary_read_all(io.BytesIO(b"")) == b""


class TestWriteReturningNone:
    def test_surfaces_as_blockingioerror(self):
        with pytest.raises(BlockingIOError, match="nothing could be written right now"):
            ext.binary_write(NoneWriter(), b"abc")

    def test_nothing_is_silently_claimed_to_have_been_written(self):
        writer = NoneWriter()
        with pytest.raises(BlockingIOError):
            ext.binary_write(writer, b"abc")
        assert writer.calls == 1


class TestRealNonBlockingPipe:
    """The same thing end to end, with a real file descriptor rather than a stand-in."""

    @pytest.fixture
    def pipe(self):
        read_fd, write_fd = os.pipe()
        os.set_blocking(read_fd, False)
        handle = open(read_fd, "rb", buffering=0)
        yield handle, write_fd
        handle.close()
        os.close(write_fd)

    def test_not_ready_then_ready(self, pipe):
        handle, write_fd = pipe
        with pytest.raises(BlockingIOError):
            ext.binary_read_exactly(handle, 16)
        os.write(write_fd, b"hello")
        assert ext.binary_read_exactly(handle, 16) == b"hello"


class TestHowTheNeighboursHandleIt:
    """`None` is where the two existing crates differ from this one most sharply."""

    def test_pyo3_file_reports_a_non_retryable_error_on_read(self):
        """It has no `None` case, so the extraction failure surfaces instead."""
        with pytest.raises(OSError) as excinfo:
            ext.legacy_read_once(NoneThenData(b"hello"))
        assert not isinstance(excinfo.value, BlockingIOError)

    def test_pyo3_file_reports_a_non_retryable_error_on_write(self):
        with pytest.raises(OSError) as excinfo:
            ext.legacy_write(NoneWriter(), b"abc")
        assert not isinstance(excinfo.value, BlockingIOError)

    def test_pyo3_filelike_claims_success_on_a_none_write(self):
        """It discards `write`'s return value, so nothing written looks like everything written."""
        writer = NoneWriter()
        assert ext.filelike_write(writer, b"abc") == 3
        assert writer.calls == 1  # and nothing actually arrived

    def test_pyo3_filelike_loses_data_on_a_partial_write(self):
        """The same discarded return value, in the case that is not about non-blocking at all."""

        class Partial:
            def __init__(self):
                self.got = b""

            def write(self, data: bytes, /) -> int:
                chunk = data[:3]
                self.got += chunk
                return len(chunk)

            def flush(self) -> None: ...

        writer = Partial()
        assert ext.filelike_write(writer, b"0123456789") == 10
        assert writer.got == b"012"  # seven bytes silently dropped

    def test_this_crate_retries_a_partial_write(self):
        class Partial:
            def __init__(self):
                self.got = b""

            def write(self, data: bytes, /) -> int:
                chunk = data[:3]
                self.got += chunk
                return len(chunk)

        writer = Partial()
        assert ext.binary_write(writer, b"0123456789") == 10
        assert writer.got == b"0123456789"
