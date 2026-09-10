from _typeshed import ReadableBuffer
from typing import Any, Protocol

class SupportsBinaryFileno(Protocol):
    def fileno(self, /) -> int: ...

class SupportsBinaryRead(Protocol):
    def read(self, size: int, /) -> ReadableBuffer: ...

class SupportsBinaryReadSeek(Protocol):
    def read(self, size: int, /) -> ReadableBuffer: ...
    def seek(self, offset: int, whence: int, /) -> int: ...

class SupportsBinaryReadWrite(Protocol):
    def flush(self, /) -> object: ...
    def read(self, size: int, /) -> ReadableBuffer: ...
    def write(self, data: bytes, /) -> int: ...

class SupportsBinaryReadWriteSeekFileno(Protocol):
    def fileno(self, /) -> int: ...
    def flush(self, /) -> object: ...
    def read(self, size: int, /) -> ReadableBuffer: ...
    def seek(self, offset: int, whence: int, /) -> int: ...
    def write(self, data: bytes, /) -> int: ...

class SupportsBinaryWrite(Protocol):
    def flush(self, /) -> object: ...
    def write(self, data: bytes, /) -> int: ...

class SupportsTextFileno(Protocol):
    def fileno(self, /) -> int: ...

class SupportsTextRead(Protocol):
    def read(self, size: int, /) -> str: ...

class SupportsTextReadSeek(Protocol):
    def read(self, size: int, /) -> str: ...
    def seek(self, offset: int, whence: int, /) -> int: ...
    def tell(self, /) -> int: ...

class SupportsTextReadWrite(Protocol):
    def flush(self, /) -> object: ...
    def read(self, size: int, /) -> str: ...
    def write(self, data: str, /) -> int: ...

class SupportsTextWrite(Protocol):
    def flush(self, /) -> object: ...
    def write(self, data: str, /) -> int: ...

def binary_everything(file: SupportsBinaryReadWriteSeekFileno, data: bytes) -> tuple[int, int]: ...
def binary_fileno(file: SupportsBinaryFileno) -> int: ...

def binary_read_all(file: SupportsBinaryRead) -> bytes:
    """
    Reads the whole stream. One `read(-1)` rather than a call per chunk.
    """

def binary_read_exactly(file: SupportsBinaryRead, n: int) -> bytes:
    """
    One `read(n)` into a fixed buffer: exactly the bytes Python handed over.
    """

def binary_read_write(file: SupportsBinaryReadWrite, data: bytes) -> int: ...

def binary_seek_roundtrip(file: SupportsBinaryReadSeek) -> bytes:
    """
    Reads, seeks back to the start and reads again, to show `Seek` working on bytes.
    """

def binary_write(file: SupportsBinaryWrite, data: bytes) -> int: ...

def legacy_fileno(obj: Any) -> int:
    """
    The untyped equivalent of [`binary_fileno`]. `pyo3-file` exposes this through `AsRawFd`,
    which cannot report failure, so it panics when `fileno()` raises.
    """

def legacy_read_all(obj: Any) -> bytes:
    """
    The untyped equivalent of [`binary_read_all`], to compare against.
    """

def legacy_read_all_text(obj: Any) -> str:
    """
    The untyped equivalent of [`text_read_all`]: it goes through `std::io::Read`, so the
    characters are encoded to UTF-8 by pyo3-file and decoded again here.
    """

def legacy_read_chars(obj: Any, n: int) -> str:
    """
    Reads `n` characters the `pyo3-file` way, for a like-for-like comparison with
    [`text_read_chars`]: a buffer four times the character count, one `read`, then a UTF-8
    decode of what `pyo3-file` encoded on the way in.
    """

def legacy_read_once(obj: Any) -> bytes:
    """
    A single fixed-size `pyo3-file` read, so the result does not depend on `read_to_end`'s
    buffer bookkeeping. Shows plainly what the Rust side receives.
    """

def legacy_write(obj: Any, data: bytes) -> int:
    """
    The untyped equivalent of [`binary_write`]. Writing non-UTF-8 bytes to a text object panics,
    because `pyo3-file` has to decode them to hand Python a `str`.
    """

def text_fileno(file: SupportsTextFileno) -> int: ...

def text_read_all(file: SupportsTextRead) -> str:
    """
    Reads the whole stream as text. One `read(-1)` and one `str` across the boundary.
    """

def text_read_chars(file: SupportsTextRead, n: int) -> str:
    """
    Reads at most `n` **characters**, the unit Python counts a text read in.
    """

def text_read_write(file: SupportsTextReadWrite, text: str) -> int: ...

def text_seek_roundtrip(file: SupportsTextReadSeek, n: int) -> tuple[str, str]:
    """
    Uses `tell` to make a cookie and `seek_to` to come back to it.
    """

def text_write(file: SupportsTextWrite, text: str) -> int: ...
