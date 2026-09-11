"""Everything here must be rejected, each for its own reason.

`run-tests.sh` asserts the exact count, so a rule that silently stops working is caught.
"""

import io

import pyo3_file_typed_tests as ext


class ReadsText:
    def read(self, size: int, /) -> str | None:
        return ""


class ReadsBytes:
    def read(self, size: int, /) -> bytes | None:
        return b""


class ReadsBytesWritesText:
    def read(self, size: int, /) -> bytes:
        return b""

    def write(self, data: str, /) -> int:
        return 0

    def flush(self) -> None: ...


class SeeksWithoutTell:
    def read(self, size: int, /) -> str:
        return ""

    def seek(self, offset: int, whence: int, /) -> int:
        return 0


# 1. a text object where bytes are wanted: the gzip case
ext.binary_read_all(io.StringIO("abc"))

# 2. the same, duck typed
ext.binary_read_all(ReadsText())

# 3. a binary object where text is wanted
ext.text_read_all(io.BytesIO(b"abc"))

# 4. the same, duck typed
ext.text_read_all(ReadsBytes())

# 5. no read at all
ext.binary_read_all(object())

# 6. reads bytes but writes text: consistent with neither kind
ext.binary_read_write(ReadsBytesWritesText(), b"x")

# 7. text seeking needs tell, because the positions are opaque cookies
ext.text_seek_roundtrip(SeeksWithoutTell(), 1)

# 8. a read-only object where writing is required
ext.binary_write(ReadsBytes(), b"x")

# 9. no fileno
ext.binary_fileno(ReadsBytes())

# 10. wrong argument type entirely
ext.binary_write(io.BytesIO(), "not bytes")
