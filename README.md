# pyo3-typed-io

Python file-like objects with **payload kind** and **required capabilities** in their Rust type. Binary streams implement the appropriate `std::io` traits; text streams have a character-counted API and opaque seek positions.

## API guide

| Task | Start with | Method reference |
|---|---|---|
| Read or write bytes | [`PyBinaryRead`], [`PyBinaryWrite`] | [`PyBinaryIO`] |
| Read or write text | [`PyTextRead`], [`PyTextWrite`] | [`PyTextIO`] |
| Read and seek | [`PyBinaryReadSeek`], [`PyTextReadSeek`] | The corresponding binary/text reference |
| Construct, bind, or access the Python object | Any owned alias | [`PyIO`] |
| Work while already attached to Python | Obtain a bound form with `bind` or `into_bound` | [`BoundPyBinaryIO`], [`BoundPyTextIO`] |

The complete owned catalog is in [`aliases`]; explicit lifetime-bearing names are in [`aliases::bound`]. Every alias is also importable from the crate root. Each convenience alias page lists **Available operations** and links to the relevant method reference, because rustdoc does not automatically copy implementations onto type aliases.

## Quickstart

```toml
[dependencies]
pyo3 = "0.29.2"
pyo3-typed-io = "0.1.0"
```

Use named aliases directly as PyO3 arguments; they implement `FromPyObject`.

```rust,no_run
use pyo3::prelude::*;
use pyo3_typed_io::{PyBinaryRead, PyTextReadSeek};
use std::io::Read;

#[pyfunction]
fn read_bytes(mut source: PyBinaryRead) -> PyResult<Vec<u8>> {
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[pyfunction]
fn preview(mut source: PyTextReadSeek) -> PyResult<String> {
    let position = source.tell()?;
    let text = source.read_chars(80)?;
    source.seek_to(&position)?;
    Ok(text)
}
```

Python callers can pass `io.BytesIO` to `read_bytes` and `io.StringIO` to `preview`, or compatible real files and duck-typed objects. The preview restores the position on success; it is not a transactional operation.

For manual construction, use `PyBinaryRead::py_new(bound_object)` or `PyBinaryRead::new(owned_object)`. A `PyIO` owns a Python reference and attaches to Python for operations. `file.bind(py)` produces a `BoundPyIO` for repeated operations while already attached; `into_bound(py)` and `unbind()` transfer between forms. Cloning a wrapper references the same Python object, not an independent stream.

## Types and construction

Aliases are spelled `Py` + `Binary` or `Text` + any of `Read`, `Write`, `Seek`, and `Fileno`, in that order — for example, `PyBinaryReadWriteSeek` or `PyTextReadFileno`. Only the requested operations exist on the value: a `PyBinaryRead` cannot be written to, and no text wrapper implements `std::io` traits. These flags are requirements, not guarantees: the object can still close, change, or fail at runtime.

Construction rejects objects of the wrong payload kind (`io.TextIOBase` vs `io.RawIOBase`/`io.BufferedIOBase`), objects missing a required method (or exposing a noncallable one), known-closed streams, and streams whose `readable()`/`writable()`/`seekable()` query refuses the capability. Anything else is taken at its word, with the actual payload validated on every read. Failures surface as `TypeError` (`ValueError` for a closed stream). There is no way to skip classification: if a class inherits from the wrong half of the `io` hierarchy, fix the base; if it is someone else's, wrap it in a plain delegating object instead of inheriting.

Choose the encoding on the Python side: open a binary stream when Rust needs the original bytes, or wrap it with `io.TextIOWrapper(..., encoding=...)` when Rust needs text.

## I/O contracts

### Binary

* `Read` calls Python `read(size)` with a bounded byte count. Python `bytes` use a borrowed `PyBytes` fast path before copying into the Rust destination. Other buffer exporters are snapshotted with `memoryview(...).tobytes()`, preserving raw buffer bytes rather than converting individual elements. A `list[int]` is not a buffer and is rejected.
* Results larger than the requested byte count are rejected. Bulk `read_to_end`/`read_to_string` keep the standard Rust contract — bounded positive-size reads (at most 64 KiB each), `Interrupted` retries, and data read before an error kept — but append straight from each returned `bytes` instead of zero-filling a buffer first; there is no universal `read(-1)` shortcut. Binary `read_to_string` performs Rust UTF-8 validation, with std's rules for invalid data.
* `Write` passes Python `bytes`, honors short-write counts, and rejects counts larger than the input. `write_all` uses the usual retry behavior.
* `Seek` uses byte-oriented `SeekFrom`; positions and offsets must fit the supported integer ranges.

### Text

* `read_chars(n)` requests at most `n` Unicode characters, not bytes or grapheme clusters. It may return fewer and rejects an overlong result.
* `read_to_string()` collects text through bounded positive-size reads. On error, consumed text is not returned; nonblocking callers should use `read_chars` and retain each successful chunk before retrying.
* `write_str()` returns the number of characters accepted. `write_all_str()` handles short writes on character boundaries and rejects zero progress or impossible counts.
* `tell()` returns a `PyTextPosition` holding an arbitrary-size Python integer. `seek_to(&position)` restores that opaque cookie; `rewind()` and `seek_to_end()` also return `PyTextPosition`. Cookies are not byte offsets and should be used with the stream that produced them, subject to its positioning rules.

Text crosses into Rust as UTF-8 `String`/`str`; this is not conversion-free. Python strings may contain surrogate values that Rust UTF-8 strings cannot represent, so not every Python `str` can be extracted successfully.

For both kinds, an empty read result is EOF; `None` from `read` or `write` means `WouldBlock`, not EOF or successful completion. Python I/O exceptions propagate through the error conversions. Invalid payloads and overlong/impossible counts are errors rather than silently truncated data. `flush()` is optional: absence is a no-op, while an existing method's failure is reported. If a Python `BlockingIOError` reports a positive `characters_written`, it becomes a validated short-write result (bytes for binary, characters for text), so callers do not resend input that Python already accepted.

## File descriptors

With `Fileno`, `fileno()` safely returns `io::Result<i32>`: a raw number, **not** a borrowed descriptor or a lifetime guarantee. Wrappers deliberately do not implement `AsFd`. The Unix compatibility `AsRawFd` implementation may panic if `fileno()` fails; prefer the fallible method.

On Unix, `try_clone_fd() -> io::Result<OwnedFd>` duplicates the descriptor with `fcntl(F_DUPFD_CLOEXEC)`. The owned, close-on-exec duplicate survives closing the Python stream. It shares the OS file offset and bypasses Python buffering, so coordinate/flush buffered I/O before mixing interfaces. Callers must synchronize concurrent close or descriptor reuse to ensure the intended resource is duplicated; a Python reference alone does not prevent that race.

## Optional Python annotations

`experimental-inspect` supplies structural capability protocols to PyO3's experimental inspection metadata. For example, a `PyTextReadSeek` argument describes `read(size) -> str | None`, `seek(offset, whence) -> int`, and `tell() -> int`.

Generating usable stubs still requires a custom `pyo3-introspection` generator with `attach_to_root` support; enabling the feature alone is not sufficient. Static protocols describe expected signatures, not stream state, payload correctness, or all dynamic attribute behavior, so static and runtime acceptance need not coincide.

## Alternatives and performance

The comparison fixtures use published `pyo3-file` **0.17.0** and `pyo3-filelike` **0.5.3**:

| Design | `pyo3-file` | `pyo3-filelike` | `pyo3-typed-io` |
|---|---|---|---|
| Payload model | One runtime-adapting wrapper | Separate binary/text wrappers | Separate binary/text types |
| Construction | Runtime capability options; also implements `FromPyObject` | Explicit constructors, mode-based classification | Typed requirements; hierarchy rejection and callable checks |
| Rust text interface | Byte-oriented I/O adapter | UTF-8 byte-oriented adapter | Character API and opaque seek cookies |
| Capabilities in Rust type | No | No | Yes |

An adapter that intentionally encodes text as UTF-8 is a legitimate design when those are the desired bytes; it does not recover the original bytes of a file decoded with another encoding. That choice is distinct from contract bugs such as mishandling short-write counts. The tests compare observable behavior rather than treating a different abstraction as inherently incorrect.

There is no universal speed ranking. Construction checks, Python attachment, Python calls per chunk, allocations, and copying all matter. Binary reads still copy into Rust, non-`bytes` buffers need a snapshot, writes construct Python payloads, and text extraction has Unicode conversion costs. Bound wrappers avoid repeated attachment when the caller already holds a Python token.

### Measured results

Median of three `python pytests/bench.py --repeats 9` runs, each figure itself the median of nine repeat means over ten calls: release build, otherwise idle Intel Core Ultra 7 258V, Linux 7.2, Python 3.14.7, measured 2026-09-23. Times are microseconds per complete operation, and every I/O row includes constructing one adapter. Factors are relative to the bound typed adapter, so above 1× is slower than it.

"fails": `pyo3-file` raises `OSError: buffer size must be at least 4 bytes` reading a whole text stream, and panics on non-ASCII text writes because it counts the characters Python reports as bytes. "n/a": `pyo3-filelike` has no text writer, and neither crate reads a count of characters. These are this machine's numbers for these workloads, not a general ranking; rerun the benchmark for yours.

| Workload | typed, bound | typed, detached | `pyo3-file` | `pyo3-filelike` |
|---|---:|---:|---:|---:|
| **Binary, `io.BytesIO`, 1 MiB** | | | | |
| construction only (read) | 0.357 | 0.355 | 0.060 (0.17×) | 0.186 (0.52×) |
| `read_to_end` | 39.4 | 38.9 | 81.2 (2.06×) | 84.4 (2.14×) |
| `read`, 64 B buffer | 372.5 | 418.6 | 408.5 (1.10×) | 757.6 (2.03×) |
| `read`, 4,096 B buffer | 31.2 | 31.9 | 31.3 (1.00×) | 37.7 (1.21×) |
| `read`, 65,536 B buffer | 34.5 | 35.7 | 33.7 (0.98×) | 34.6 (1.00×) |
| `write_all` 1,048,576 B × 1 | 54.6 | 54.9 | 53.3 (0.98×) | 54.3 (0.99×) |
| `write_all` 64 B × 16,384 | 446.9 | 490.6 | 466.2 (1.04×) | 804.3 (1.80×) |
| `write_all` 4,096 B × 256 | 35.0 | 35.5 | 33.9 (0.97×) | 40.5 (1.16×) |
| `write_all` 65,536 B × 16 | 32.9 | 34.5 | 32.1 (0.97×) | 33.0 (1.00×) |
| `write_all` 64 B × 16,384, then `flush` | 448.5 | 494.3 | 467.5 (1.04×) | 808.9 (1.80×) |
| **Binary, buffered temporary file, 1 MiB** | | | | |
| construction only (read) | 0.383 | 0.382 | 0.060 (0.16×) | 0.052 (0.14×) |
| `read_to_end` | 96.0 | 96.5 | 127.9 (1.33×) | 128.3 (1.34×) |
| `read`, 64 B buffer | 439.5 | 491.1 | 482.5 (1.10×) | 832.3 (1.89×) |
| `read`, 4,096 B buffer | 67.5 | 68.7 | 67.4 (1.00×) | 74.0 (1.10×) |
| `read`, 65,536 B buffer | 70.3 | 71.3 | 69.4 (0.99×) | 71.6 (1.02×) |
| `write_all` 1,048,576 B × 1 | 125.3 | 126.2 | 124.7 (1.00×) | 123.4 (0.98×) |
| `write_all` 64 B × 16,384 | 800.3 | 865.3 | 841.4 (1.05×) | 1,191.3 (1.49×) |
| `write_all` 4,096 B × 256 | 115.5 | 116.1 | 114.5 (0.99×) | 120.9 (1.05×) |
| `write_all` 65,536 B × 16 | 112.6 | 113.0 | 112.2 (1.00×) | 113.7 (1.01×) |
| `write_all` 64 B × 16,384, then `flush` | 809.0 | 877.5 | 851.9 (1.05×) | 1,204.5 (1.49×) |
| **Text, ASCII, 1 Mi characters, `io.StringIO`** | | | | |
| read whole stream | 161.8 | 166.2 | fails | 362.7 (2.24×) |
| `read_chars(64)` until EOF | 644.1 | 696.0 | n/a | n/a |
| `read_chars(4,096)` until EOF | 174.9 | 176.6 | n/a | n/a |
| `read_chars(65,536)` until EOF | 210.7 | 211.1 | n/a | n/a |
| write 1,048,576 chars × 1 | 77.4 | 77.0 | 99.2 (1.28×) | n/a |
| write 64 chars × 16,384 | 554.6 | 626.2 | 629.5 (1.14×) | n/a |
| write 4,096 chars × 256 | 64.3 | 65.3 | 75.3 (1.17×) | n/a |
| write 65,536 chars × 16 | 71.0 | 71.1 | 89.3 (1.26×) | n/a |
| **Text, Unicode, 1 Mi characters, `io.StringIO`** | | | | |
| read whole stream | 666.5 | 670.1 | fails | 1,660.3 (2.49×) |
| `read_chars(64)` until EOF | 1,584.9 | 1,686.8 | n/a | n/a |
| `read_chars(4,096)` until EOF | 711.8 | 716.9 | n/a | n/a |
| `read_chars(65,536)` until EOF | 729.8 | 730.2 | n/a | n/a |
| write 1,048,576 chars × 1 | 745.8 | 744.9 | fails | n/a |
| write 64 chars × 16,384 | 2,057.7 | 2,123.6 | fails | n/a |
| write 4,096 chars × 256 | 849.8 | 852.6 | fails | n/a |
| write 65,536 chars × 16 | 806.9 | 812.0 | fails | n/a |

In summary: construction checks more and costs about 0.3 µs more. Bulk `read_to_end` is 1.3–2.1× faster than either crate, because it appends each returned `bytes` directly instead of zero-filling growing buffers. Whole-stream text reads are 2.2–2.5× faster than `pyo3-filelike`. With 4 KiB or larger chunks, binary I/O is within noise of both crates: Python allocating and copying the payload dominates. With 64-byte chunks it matches `pyo3-file` and is 1.5–2× faster than `pyo3-filelike`. ASCII text writes are 1.14–1.28× faster than `pyo3-file`.

Peak process memory growth for one binary `read_to_end` of 16 MiB (`python pytests/bench.py --memory --size 16777216`, one fresh process per case). Growth is the peak minus the high-water mark before the read, so it includes the 16 MiB result itself. The typed reads ask Python for at most 64 KiB at a time and append each result, rather than doubling a zero-filled buffer:

| Source | typed, bound | typed, detached | `pyo3-file` | `pyo3-filelike` |
|---|---:|---:|---:|---:|
| `io.BytesIO` | +18.0 MiB | +16.6 MiB | +32.5 MiB | +32.4 MiB |
| buffered temporary file | +18.2 MiB | +16.8 MiB | +32.4 MiB | +32.7 MiB |

## Validation and benchmarks

See [pytests/README.md](pytests/README.md) for how to run the test suite and benchmarks.
