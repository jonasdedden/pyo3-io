"""Binary payloads: the bytes Rust sees are exactly the bytes Python produced."""

import io
import os
import tempfile

import pytest
from helpers import (
    DuckBinaryReader,
    DuckBinaryWriter,
    LiesAboutWrite,
    NonBlocking,
    Overreader,
    PartialWriter,
)

import pyo3_file_typed_tests as ext

# Deliberately not valid UTF-8, so anything that decodes on the way through is caught.
LATIN1 = bytes([0xE9, 0xE8, 0xFC]) * 8
GZIP_MAGIC = bytes([0x1F, 0x8B, 0x08, 0x00, 0xFF, 0xFE])


@pytest.fixture
def tmp_binary(tmp_path):
    path = tmp_path / "data.bin"
    path.write_bytes(LATIN1)
    return path


class TestReadAll:
    def test_bytesio(self):
        assert ext.binary_read_all(io.BytesIO(LATIN1)) == LATIN1

    def test_real_file(self, tmp_binary):
        with open(tmp_binary, "rb") as handle:
            assert ext.binary_read_all(handle) == LATIN1

    def test_unbuffered_file(self, tmp_binary):
        with open(tmp_binary, "rb", buffering=0) as handle:
            assert ext.binary_read_all(handle) == LATIN1

    def test_duck_typed(self):
        assert ext.binary_read_all(DuckBinaryReader(LATIN1)) == LATIN1

    def test_empty(self):
        assert ext.binary_read_all(io.BytesIO(b"")) == b""

    def test_large(self):
        data = os.urandom(1 << 20)
        assert ext.binary_read_all(io.BytesIO(data)) == data

    def test_every_byte_value_round_trips(self):
        data = bytes(range(256))
        assert ext.binary_read_all(io.BytesIO(data)) == data

    def test_embedded_nulls(self):
        data = b"a\x00b\x00\x00c"
        assert ext.binary_read_all(io.BytesIO(data)) == data

    def test_gzip_magic_survives(self):
        """The case that motivated the split: a binary stream must not be decoded."""
        assert ext.binary_read_all(io.BytesIO(GZIP_MAGIC)) == GZIP_MAGIC

    def test_read_to_end_is_one_call(self):
        """`read(-1)` once, then one empty call to confirm the end. Not one call per chunk."""
        reader = DuckBinaryReader(os.urandom(1 << 16))
        ext.binary_read_all(reader)
        assert reader.calls == 2


class TestReadExactly:
    @pytest.mark.parametrize("n", [0, 1, 2, 3, 4, 5, 23, 24, 25, 1000])
    def test_sizes(self, n):
        """No minimum buffer size: `n` bytes requested is `n` bytes counted."""
        got = ext.binary_read_exactly(io.BytesIO(LATIN1), n)
        assert got == LATIN1[:n]

    def test_size_is_bytes_not_items(self):
        assert len(ext.binary_read_exactly(io.BytesIO(LATIN1), 3)) == 3

    def test_past_end(self):
        assert ext.binary_read_exactly(io.BytesIO(b"ab"), 100) == b"ab"


class TestWrite:
    def test_bytesio(self):
        buffer = io.BytesIO()
        assert ext.binary_write(buffer, LATIN1) == len(LATIN1)
        assert buffer.getvalue() == LATIN1

    def test_duck_typed(self):
        writer = DuckBinaryWriter()
        ext.binary_write(writer, LATIN1)
        assert writer.value == LATIN1
        assert writer.flushes == 1

    def test_non_utf8_bytes(self):
        """pyo3-file panics here when the object is a text stream; there is no such path now."""
        buffer = io.BytesIO()
        ext.binary_write(buffer, GZIP_MAGIC)
        assert buffer.getvalue() == GZIP_MAGIC

    def test_partial_writes_are_retried(self):
        writer = PartialWriter()
        ext.binary_write(writer, b"0123456789")
        assert writer.value == b"0123456789"

    def test_real_file(self, tmp_path):
        path = tmp_path / "out.bin"
        with open(path, "wb") as handle:
            ext.binary_write(handle, LATIN1)
        assert path.read_bytes() == LATIN1

    def test_empty(self):
        buffer = io.BytesIO()
        assert ext.binary_write(buffer, b"") == 0


class TestSeek:
    def test_roundtrip(self):
        assert ext.binary_seek_roundtrip(io.BytesIO(LATIN1)) == LATIN1

    def test_real_file(self, tmp_binary):
        with open(tmp_binary, "rb") as handle:
            assert ext.binary_seek_roundtrip(handle) == LATIN1


class TestFileno:
    def test_real_file(self, tmp_binary):
        with open(tmp_binary, "rb") as handle:
            assert ext.binary_fileno(handle) == handle.fileno()

    def test_unsupported_is_an_error_not_a_panic(self):
        """`io.BytesIO` has a `fileno` that raises. pyo3-file's `AsRawFd` panics on this."""
        with pytest.raises(OSError) as excinfo:
            ext.binary_fileno(io.BytesIO(b"abc"))
        assert "pyo3_runtime.PanicException" not in type(excinfo.value).__name__


class TestCombined:
    def test_everything(self, tmp_path):
        path = tmp_path / "rw.bin"
        path.write_bytes(b"")
        with open(path, "w+b") as handle:
            size, fd = ext.binary_everything(handle, LATIN1)
        assert size == len(LATIN1)
        assert fd > 0
        assert path.read_bytes() == LATIN1

    def test_read_write(self):
        buffer = io.BytesIO()
        assert ext.binary_read_write(buffer, LATIN1) == 0  # cursor is at the end


class TestMisbehavingObjects:
    def test_returning_more_than_asked(self):
        with pytest.raises(OSError, match="after being asked for at most"):
            ext.binary_read_exactly(Overreader(), 4)

    def test_none_is_would_block(self):
        with pytest.raises(OSError, match="no data is available yet"):
            ext.binary_read_exactly(NonBlocking(), 4)

    def test_write_claiming_too_much(self):
        with pytest.raises(OSError, match="reported"):
            ext.binary_write(LiesAboutWrite(), b"abc")


class TestBufferReturnTypes:
    """The protocol says `ReadableBuffer`, and every one of these is accepted at runtime."""

    @staticmethod
    def _reader(value):
        class Reader:
            def __init__(self) -> None:
                self.done = False

            def read(self, size: int = -1, /):
                if self.done:
                    return b""
                self.done = True
                return value

        return Reader()

    @pytest.mark.parametrize(
        "value",
        [b"abc", bytearray(b"abc"), memoryview(b"abc")],
        ids=["bytes", "bytearray", "memoryview"],
    )
    def test_any_buffer_is_accepted(self, value):
        assert ext.binary_read_all(self._reader(value)) == b"abc"

    def test_text_streams_buffer_attribute(self, tmp_path):
        """The escape hatch the payload-kind error points at has to actually work."""
        path = tmp_path / "t.txt"
        path.write_bytes(LATIN1)
        with open(path, encoding="latin-1") as handle:
            assert ext.binary_read_all(handle.buffer) == LATIN1
