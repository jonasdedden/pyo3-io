# pyo3-file-typed

Python file-like objects with **payload kind** and **required capabilities** in their Rust type.
Binary streams implement the appropriate `std::io` traits; text streams have a
character-counted API and opaque seek positions.

## Quickstart

```toml
[dependencies]
pyo3 = "0.29.2"
pyo3-file-typed = "0.1.0"
```

Use named aliases directly as PyO3 arguments; they implement `FromPyObject`.

```rust,no_run
use pyo3::prelude::*;
use pyo3_file_typed::{BinaryRead, TextReadSeek};
use std::io::Read;

#[pyfunction]
fn read_bytes(mut source: BinaryRead) -> PyResult<Vec<u8>> {
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?;
    Ok(bytes)
}

#[pyfunction]
fn preview(mut source: TextReadSeek) -> PyResult<String> {
    let position = source.tell()?;
    let text = source.read_chars(80)?;
    source.seek_to(&position)?;
    Ok(text)
}
```

Python callers can pass `io.BytesIO` to `read_bytes` and `io.StringIO` to `preview`,
or compatible real files and duck-typed objects. The preview restores the position
on success; it is not a transactional operation.

For manual construction, use `BinaryRead::py_new(bound_object)` or
`BinaryRead::new(owned_object)`. A `PyFile` owns a Python reference and attaches to
Python for operations. `file.bind(py)` produces a `BoundFile` for repeated operations
while already attached; `into_bound(py)` and `unbind()` transfer between forms.
Cloning a wrapper references the same Python object, not an independent stream.

## Types and construction

The aliases combine `Binary` or `Text` with `Read`, `Write`, `Seek`, and `Fileno`,
in that order: for example, `BinaryReadWriteSeek` and `TextReadFileno`.
All 30 nonempty combinations, plus their `Bound…<'py>` forms, are defined in the
shared `src/file_types.rs` macro table and available without optional features.
The underlying types are `PyBinaryFile<READ, WRITE, SEEK, FILENO>` and
`PyTextFile<READ, WRITE, SEEK, FILENO>`.

Only requested operations are exposed in Rust. `BinaryRead` does not implement
`Write`, and no text wrapper implements `std::io::Read`, `Write`, or `Seek`.
These flags express **requirements**, not permanent proof that Python will satisfy
them: objects can close, change methods, return bad payloads, or fail during I/O.

Construction applies these rules:

* **Reject known opposite payload kinds.** `io.TextIOBase` identifies text;
  `io.RawIOBase` and `io.BufferedIOBase` identify binary. `io.IOBase` alone does
  not identify either kind. Unknown pure ducks are accepted subject to capability
  checks, with payload validation on reads.
* **Require callable methods through normal attribute lookup.** Instance methods,
  descriptors, and `__getattr__` can supply them. Registration with `io.Reader`
  or `io.Writer` is not a substitute for a callable `read` or `write`.
  Text seeking requires both `seek` and `tell`.
* **Check known `io.IOBase` state.** A known closed stream raises `ValueError`.
  For requested directions, a strict `False` from `readable()`, `writable()`, or
  `seekable()` rejects the capability. Pure ducks do not need these queries.

There is no `.mode`, `.encoding`, or `read(0)` probing. Construction still can run
Python code through lookup, hierarchy checks, and state queries; it is not
side-effect-free. Missing/noncallable methods, known wrong kinds, and refused
capabilities become Python `TypeError`s.

`py_new_unchecked` and the `Unchecked<T>` argument adapter skip only payload-kind
classification, not capability checks or runtime payload/count validation. They
are safe escape hatches for misleading inheritance, not unchecked memory access.

Choose encoding on the Python side: open a binary stream when Rust needs original
bytes, or wrap it with `io.TextIOWrapper(..., encoding=...)` when Rust needs text.
Some text streams expose `.buffer`, but not all do, and mixing that interface with
buffered text operations requires care.

## I/O contracts

### Binary

* `Read` calls Python `read(size)` with a bounded byte count. Python `bytes` use
  a borrowed `PyBytes` fast path before copying into the Rust destination.
  Other buffer exporters are snapshotted with `memoryview(...).tobytes()`, preserving
  raw buffer bytes rather than converting individual elements. A `list[int]` is
  not a buffer and is rejected.
* Results larger than the requested byte count are rejected. Bulk
  `read_to_end`/`read_to_string` use the standard Rust bounded-read defaults,
  including `Interrupted` retries; there is no universal `read(-1)` shortcut.
  Binary `read_to_string` performs Rust UTF-8 validation.
* `Write` passes Python `bytes`, honors short-write counts, and rejects counts
  larger than the input. `write_all` uses the usual retry behavior.
* `Seek` uses byte-oriented `SeekFrom`; positions and offsets must fit the
  supported integer ranges.

### Text

* `read_chars(n)` requests at most `n` Unicode characters, not bytes or grapheme
  clusters. It may return fewer and rejects an overlong result.
* `read_to_string()` collects text through bounded positive-size reads. On error,
  consumed text is not returned; nonblocking callers should use `read_chars` and
  retain each successful chunk before retrying.
* `write_str()` returns the number of characters accepted. `write_all_str()` handles
  short writes on character boundaries and rejects zero progress or impossible counts.
* `tell()` returns a `TextPosition` holding an arbitrary-size Python integer.
  `seek_to(&position)` restores that opaque cookie; `rewind()` and `seek_to_end()`
  also return `TextPosition`. Cookies are not byte offsets and should be used with
  the stream that produced them, subject to its positioning rules.

Text crosses into Rust as UTF-8 `String`/`str`; this is not conversion-free.
Python strings may contain surrogate values that Rust UTF-8 strings cannot represent,
so not every Python `str` can be extracted successfully.

For both kinds, an empty read result is EOF; `None` from `read` or `write` means
`WouldBlock`, not EOF or successful completion. Python I/O exceptions propagate
through the error conversions. Invalid payloads and overlong/impossible counts
are errors rather than silently truncated data. `flush()` is optional: absence
is a no-op, while an existing method's failure is reported.
If a Python `BlockingIOError` reports a positive `characters_written`, it becomes
a validated short-write result (bytes for binary, characters for text), so callers
do not resend input that Python already accepted.

## File descriptors

With `Fileno`, `fileno()` safely returns `io::Result<i32>`: a raw number, **not**
a borrowed descriptor or a lifetime guarantee. Wrappers deliberately do not
implement `AsFd`. The Unix compatibility `AsRawFd` implementation may panic if
`fileno()` fails; prefer the fallible method.

On Unix, `try_clone_fd() -> io::Result<OwnedFd>` duplicates the descriptor with
`fcntl(F_DUPFD_CLOEXEC)`. The owned, close-on-exec duplicate survives closing the
Python stream. It shares the OS file offset and bypasses Python buffering, so
coordinate/flush buffered I/O before mixing interfaces. Callers must synchronize
concurrent close or descriptor reuse to ensure the intended resource is duplicated;
a Python reference alone does not prevent that race.

## Optional Python annotations

`experimental-inspect` supplies structural capability protocols to PyO3's
experimental inspection metadata. For example, a `TextReadSeek` argument describes
`read(size) -> str | None`, `seek(offset, whence) -> int`, and `tell() -> int`.

The same macro table drives the aliases, protocol declarations and annotation lookup.
Macros emit uniquely named metadata statics; a small `const` encoder supplies their
class/function envelopes and delegates type expressions to PyO3's serializers.
There is no build script, generated Rust source, or `OUT_DIR` include. The encoder
uses experimental, implementation-level PyO3 APIs isolated in the introspection module.

Generating usable stubs still requires a custom `pyo3-introspection` generator
with `attach_to_root` support; enabling the feature alone is not sufficient.
The library uses published dependencies and has no local-path dependency.
Static protocols describe expected signatures, not stream state, payload correctness,
or all dynamic attribute behavior, so static and runtime acceptance need not coincide.

## Alternatives and performance

The comparison fixtures use published `pyo3-file` **0.17.0** and
`pyo3-filelike` **0.5.3**:

| Design | `pyo3-file` | `pyo3-filelike` | `pyo3-file-typed` |
|---|---|---|---|
| Payload model | One runtime-adapting wrapper | Separate binary/text wrappers | Separate binary/text types |
| Construction | Runtime capability options; also implements `FromPyObject` | Explicit constructors, mode-based classification | Typed requirements; hierarchy rejection and callable checks |
| Rust text interface | Byte-oriented I/O adapter | UTF-8 byte-oriented adapter | Character API and opaque seek cookies |
| Capabilities in Rust type | No | No | Yes |

An adapter that intentionally encodes text as UTF-8 is a legitimate design when
those are the desired bytes; it does not recover the original bytes of a file
decoded with another encoding. That choice is distinct from contract bugs such
as mishandling short-write counts. The tests compare observable behavior rather
than treating a different abstraction as inherently incorrect.

There is no universal speed ranking. Construction checks, Python attachment,
Python calls per chunk, allocations, and copying all matter. Binary reads still
copy into Rust, non-`bytes` buffers need a snapshot, writes construct Python
payloads, and text extraction has Unicode conversion costs. Bound wrappers avoid
repeated attachment when the caller already holds a Python token.

## Validation and benchmarks

Run from the repository root with Rust, Python, and `uv` available:

```sh
cargo test --doc
./pytests/run-tests.sh
```

The runner defaults to standard Rust and Python validation, without a custom stub
generator. Opt in to stub generation/type checking or benchmarks:

```sh
PYO3_INTROSPECTION=/path/to/pyo3-introspection ./pytests/run-tests.sh --stubs
# Alternatively supply PYO3_CHECKOUT=/path/to/custom/pyo3 for the generator.
./pytests/run-tests.sh --bench
python pytests/bench.py --quick
python pytests/bench.py --profile release
python pytests/bench.py --memory --size 8388608
```

`--stubs` requires the custom generator explicitly; there is no neighboring-checkout
default. Stub checking also needs `pyright`. The Python benchmark defaults to a
release build (built incrementally outside timing) and reports median and spread for
constructor-only, chunk, bulk, real-file, and ASCII/Unicode workloads. Use its results for the measured workload,
not as a fixed speed claim.
`--memory` runs each binary bulk-read case in a fresh subprocess and reports peak
process RSS on Linux/macOS. It includes Python, extension loading and input setup;
it is not an allocated-byte count or a claim about the adapter alone.

### Migration notes

* Replace text `std::io` calls with `read_chars`, `write_str`/`write_all_str`, and
  the cookie API. Keep byte consumers on binary wrappers.
* Keep `tell()` results as `TextPosition` and pass references to `seek_to`; do not
  narrow cookies to `u64` or perform offset arithmetic on them.
* Replace borrowed-FD assumptions with fallible `fileno()` or Unix
  `try_clone_fd()`, accounting for shared offsets and Python buffering.
* A registered protocol/ABC or a noncallable attribute no longer substitutes for
  an actual callable method. Unknown ducks remain supported.
