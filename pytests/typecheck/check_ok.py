"""Everything here must type-check.

The protocols are structural contracts: matching method signatures is sufficient
for static acceptance, not a guarantee that runtime I/O succeeds.
"""

import io

import pyo3_file_typed_tests as ext

# ---- real io objects, binary
with open("/etc/hostname", "rb") as binary:
    ext.binary_read_all(binary)
    ext.binary_read_exactly(binary, 8)
    ext.binary_seek_roundtrip(binary)
    ext.binary_fileno(binary)

ext.binary_read_all(io.BytesIO(b"abc"))
ext.binary_write(io.BytesIO(), b"abc")
ext.binary_read_write(io.BytesIO(), b"abc")

with open("/tmp/rw.bin", "w+b") as both:
    ext.binary_everything(both, b"abc")

# FileIO signatures permit None for nonblocking reads and writes.
with io.FileIO("/tmp/raw.bin", "w+") as raw:
    ext.binary_read_all(raw)
    ext.binary_write(raw, b"abc")
    ext.binary_read_write(raw, b"abc")
    ext.binary_everything(raw, b"abc")

# ---- real io objects, text
with open("/etc/hostname") as text:
    ext.text_read_all(text)
    ext.text_read_chars(text, 4)
    ext.text_seek_roundtrip(text, 4)
    ext.text_fileno(text)

ext.text_read_all(io.StringIO("abc"))
ext.text_write(io.StringIO(), "abc")
ext.text_read_write(io.StringIO(), "abc")

# ---- the escape hatches between the two kinds
with open("/etc/hostname") as text2:
    ext.binary_read_all(text2.buffer)
with open("/etc/hostname", "rb") as binary2:
    ext.text_read_all(io.TextIOWrapper(binary2, encoding="utf-8"))


# ---- duck typing: structure is enough
class DuckBinaryReader:
    def read(self, size: int, /) -> bytes:
        return b""


class DuckTextReader:
    def read(self, size: int, /) -> str:
        return ""


class DuckBinaryWriter:
    """No `flush`: it is called only when present, so the protocol does not demand one."""

    def write(self, data: bytes, /) -> int:
        return 0


class DuckBinaryWriterWithFlush:
    def write(self, data: bytes, /) -> int:
        return 0

    def flush(self) -> None: ...


class DuckTextSeeker:
    def read(self, size: int, /) -> str:
        return ""

    def seek(self, offset: int, whence: int, /) -> int:
        return 0

    def tell(self) -> int:
        return 0


ext.binary_read_all(DuckBinaryReader())
ext.text_read_all(DuckTextReader())
ext.binary_write(DuckBinaryWriter(), b"x")
ext.binary_write(DuckBinaryWriterWithFlush(), b"x")
ext.text_seek_roundtrip(DuckTextSeeker(), 1)


# ---- a wider object satisfies a narrower requirement
class Everything:
    def read(self, size: int, /) -> bytes:
        return b""

    def write(self, data: bytes, /) -> int:
        return 0

    def seek(self, offset: int, whence: int, /) -> int:
        return 0

    def fileno(self) -> int:
        return 0


ext.binary_read_all(Everything())
ext.binary_fileno(Everything())
ext.binary_everything(Everything(), b"x")


# ---- nonblocking custom streams, with no optional flush method
class NonblockingBinary:
    def read(self, size: int, /) -> bytearray | None:
        return None

    def write(self, data: bytes, /) -> int | None:
        return None


class NonblockingText:
    def read(self, size: int, /) -> str | None:
        return None

    def write(self, data: str, /) -> int | None:
        return None


ext.binary_read_all(NonblockingBinary())
ext.binary_write(NonblockingBinary(), b"x")
ext.binary_read_write(NonblockingBinary(), b"x")
ext.text_read_all(NonblockingText())
ext.text_write(NonblockingText(), "x")
ext.text_read_write(NonblockingText(), "x")
