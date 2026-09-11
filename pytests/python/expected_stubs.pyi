from _typeshed import ReadableBuffer
from typing import Any, Protocol

class SupportsBinaryFileno(Protocol):
    def fileno(self, /) -> int: ...

class SupportsBinaryRead(Protocol):
    def read(self, size: int, /) -> ReadableBuffer | None: ...

class SupportsBinaryReadSeek(Protocol):
    def read(self, size: int, /) -> ReadableBuffer | None: ...
    def seek(self, offset: int, whence: int, /) -> int: ...

class SupportsBinaryReadWrite(Protocol):
    def read(self, size: int, /) -> ReadableBuffer | None: ...
    def write(self, data: bytes, /) -> int | None: ...

class SupportsBinaryReadWriteSeekFileno(Protocol):
    def fileno(self, /) -> int: ...
    def read(self, size: int, /) -> ReadableBuffer | None: ...
    def seek(self, offset: int, whence: int, /) -> int: ...
    def write(self, data: bytes, /) -> int | None: ...

class SupportsBinaryWrite(Protocol):
    def write(self, data: bytes, /) -> int | None: ...

class SupportsTextFileno(Protocol):
    def fileno(self, /) -> int: ...

class SupportsTextRead(Protocol):
    def read(self, size: int, /) -> str | None: ...

class SupportsTextReadSeek(Protocol):
    def read(self, size: int, /) -> str | None: ...
    def seek(self, offset: int, whence: int, /) -> int: ...
    def tell(self, /) -> int: ...

class SupportsTextReadWrite(Protocol):
    def read(self, size: int, /) -> str | None: ...
    def write(self, data: str, /) -> int | None: ...

class SupportsTextWrite(Protocol):
    def write(self, data: str, /) -> int | None: ...

def bench_construct(obj: Any, implementation: str, count: int) -> None:
    """
    Measures only construction and destruction, without performing an I/O operation.
    """

def bench_read(obj: Any, implementation: str, chunk: int, verify: bool) -> tuple[int, bytes | None]:
    """
    chunk=0 uses read_to_end; positive chunks reuse one fixed buffer.
    verify=true copies the complete result, for untimed correctness checks only.
    """

def bench_text_read(obj: Any, implementation: str) -> str:
    """
    Whole text streams only: the unit of work is the identical Unicode string.
    Legacy/filelike UTF-8 conversion is part of their adaptation cost.
    """

def bench_write(obj: Any, implementation: str, data: bytes, count: int, flush: bool) -> int:
    """
    Constructs once and reuses the borrowed input slice; flush is identical for all paths.
    """

def binary_everything(file: SupportsBinaryReadWriteSeekFileno, data: bytes) -> tuple[int, int]: ...
def binary_fileno(file: SupportsBinaryFileno) -> int: ...

def binary_fileno_via_as_raw_fd(file: SupportsBinaryFileno) -> int:
    """
    Raw descriptor compatibility; this does not promise a borrowed descriptor lifetime.
    """

def binary_read_all(file: SupportsBinaryRead) -> bytes:
    """
    Reads the whole stream with bounded reads and standard Rust retry semantics.
    """

def binary_read_all_bound(file: SupportsBinaryRead) -> bytes:
    """
    The same read with the token held throughout: no attaching happens inside the loop.
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

def filelike_fileno(obj: Any) -> int:
    """
    `pyo3-filelike` exposes the descriptor through `AsFd`, which cannot report failure.
    """

def filelike_read_all(obj: Any) -> bytes:
    """
    `pyo3-filelike` splits binary and text into two types, so this is the closest equivalent of
    [`binary_read_all`]. `PyBinaryFile::new` is private, so `From` is the only way in and its
    `unwrap` turns a rejected file into a panic.
    """

def filelike_read_once(obj: Any) -> bytes:
    """
    A single fixed-size read through `PyBinaryFile`.
    """

def filelike_text_as_bytes(obj: Any) -> bytes:
    """
    `PyTextFile` implements `std::io::Read`, so a text object still reaches a bytes-oriented
    consumer, re-encoded as UTF-8. This is what that consumer receives.
    """

def filelike_text_read_all(obj: Any) -> str:
    """
    Whole-stream read through `PyTextFile`, for the benchmark.
    """

def filelike_write(obj: Any, data: bytes) -> int:
    """
    `PyBinaryFile` implements `Write` unconditionally, whatever the caller asked for.
    """

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

def read_on_another_thread(file: SupportsBinaryRead) -> int:
    """
    Moves the file onto a thread of its own, with this thread releasing the GIL entirely.

    The child holds no token; each read attaches for as long as it needs and no longer. A
    GIL-bound form could not leave this thread at all.
    """

def text_fileno(file: SupportsTextFileno) -> int: ...

def text_read_all(file: SupportsTextRead) -> str:
    """
    Reads the whole stream as text using bounded character reads.
    """

def text_read_all_unchecked(file: SupportsTextRead) -> str:
    """
    Uses the escape hatch: no payload-kind check, capability checks kept.
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
