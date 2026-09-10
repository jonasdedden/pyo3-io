"""Everything here must type-check.

Nothing inherits from anything: the protocols are structural, so having the right methods with
the right payload type is the whole requirement.
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
ext.text_seek_roundtrip(DuckTextSeeker(), 1)


# ---- a wider object satisfies a narrower requirement
class Everything:
    def read(self, size: int, /) -> bytes:
        return b""

    def write(self, data: bytes, /) -> int:
        return 0

    def flush(self) -> None: ...

    def seek(self, offset: int, whence: int, /) -> int:
        return 0

    def fileno(self) -> int:
        return 0


ext.binary_read_all(Everything())
ext.binary_fileno(Everything())
ext.binary_everything(Everything(), b"x")
