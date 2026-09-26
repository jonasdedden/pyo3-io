# pyo3-io

Accept Python file objects in Rust, **typed by what they hold and what you need from them**.

```rust,no_run
use pyo3::prelude::*;
use pyo3_io::{PyBinaryRead, PyTextWrite};
use std::io::Read;

#[pyfunction]
fn checksum(mut source: PyBinaryRead) -> PyResult<u32> {
    let mut bytes = Vec::new();
    source.read_to_end(&mut bytes)?; // it's a std::io::Read
    Ok(bytes.iter().map(|&b| u32::from(b)).sum())
}

#[pyfunction]
fn greet(mut out: PyTextWrite, name: &str) -> PyResult<()> {
    out.write_all_str(&format!("Hello, {name}!\n"))?; // text in, text out
    Ok(())
}
```

```python
# ✅ the right mode and capability
checksum(open("data.bin", "rb"))
checksum(io.BytesIO(b"abc"))
greet(sys.stdout, "world")
greet(io.StringIO(), "world")

# ❌ text where bytes are expected, and the other way round
checksum(open("data.txt"))        # TypeError: expected a binary file-like object, got the text object TextIOWrapper ...
greet(io.BytesIO(), "world")      # TypeError: expected a text file-like object, got the binary object BytesIO ...

# ❌ missing capability: writing to a read-only file, reading a write-only one
greet(open("data.txt"), "world")  # TypeError: object of type TextIOWrapper reports writable() is False ...
checksum(open("out.bin", "wb"))   # TypeError: object of type BufferedWriter reports readable() is False ...

# ❌ not a file at all, or already closed
checksum(object())                # TypeError: object of type object has no .read() method ...
closed = open("data.bin", "rb")
closed.close()
checksum(closed)                  # ValueError: I/O operation on closed file
```

## Comparison to `pyo3-file` and `pyo3-filelike`

Both work well for the common case: a binary file opened with `"rb"`. The differences show up at
the edges: text streams, wrong arguments, objects without a file descriptor.

|  | **pyo3-io** | [`pyo3-file`](https://crates.io/crates/pyo3-file) 0.17 | [`pyo3-filelike`](https://crates.io/crates/pyo3-filelike) 0.5 |
|---|:---:|:---:|:---:|
| **Correctness** | | | |
| Read an `io.StringIO` | ✅ | ❌ fails for most lengths | ✅ |
| Write non-ASCII text | ✅ | 💥 panics | — no text writer |
| Text file passed where bytes are expected | ✅ `TypeError` up front | ⚠️ silently re-encoded to UTF-8 | 💥 panics, or fails on first read |
| Seek in a text stream | ✅ `tell` / `seek_to` | ⚠️ as byte offsets | — |
| **Stability** | | | |
| `fileno()` on an object without one | ✅ `OSError` | 💥 panics | 💥 panics |
| Bytes written to a text stream | ✅ `TypeError` up front | 💥 panics on non-UTF-8 | — |
| **Type safety** | | | |
| Compiler rejects Rust code that treats text as bytes | ✅ | ❌ | ✅ |
| Compiler rejects Rust code that writes to a read-only argument | ✅ | ❌ | ❌ |
| Typed Python stubs | ✅ a precise `Protocol` per signature (opt-in) | `Any` | `Any` |
| **Features** | | | |
| `std::io::{Read, Write, Seek}` for binary | ✅ | ✅ | ✅ |
| Text API counted in characters | ✅ `read_chars`, `write_str` | ❌ | ❌ |
| **Performance** (1 MiB, vs. pyo3-io) | | | |
| `read_to_end` | ✅ | 1.3–2.1× slower | 1.3–2.1× slower |
| Peak memory, 16 MiB `read_to_end` | ✅ +18 MiB | +32 MiB | +32 MiB |
| Small (64 B) reads and writes | ✅ | ✅ about the same | 1.5–2× slower |
| Text: read a whole stream | ✅ | ❌ fails | 2.2–2.5× slower |
| Constructing a wrapper | 🐌 0.36 µs (type and mode checks at creation) | ✅ 0.06 µs | ✅ 0.05–0.19 µs |

The failure cases are pinned by tests that run the crates side by side
([`test_vs_pyo3_file.py`](pytests/python/test_vs_pyo3_file.py),
[`test_vs_pyo3_filelike.py`](pytests/python/test_vs_pyo3_filelike.py)) and fail if either crate
changes its behaviour. Performance numbers come from `pytests/bench.py`; the full results are in
[pytests/README.md](pytests/README.md#measured-results).

## Usage

### Pick a type by name

Every type name is built from three parts:

```text
Py ─┬─ Binary ─┬── Read ── Write ── Seek ── Fileno
    └─ Text ───┘   ╰─── one or more, in order ───╯
    exactly one
```

So `PyBinaryRead`, `PyTextReadSeek`, `PyBinaryReadWriteSeek`, `PyTextWriteFileno`, … all exist.
Only the methods you asked for are available. The full list is in [`aliases`].

All of them work directly as `#[pyfunction]` arguments. Python callers can pass real files,
`io.BytesIO`/`io.StringIO`, or any object with the right methods.

### Binary streams are `std::io`

Binary wrappers implement `Read`, `Write` and `Seek`, so they plug into any Rust crate that
takes a reader or writer:

```rust,no_run
use pyo3::prelude::*;
use pyo3_io::{PyBinaryRead, PyBinaryWrite};
use std::io::{BufRead, BufReader, Write};

#[pyfunction]
fn count_lines(source: PyBinaryRead) -> PyResult<usize> {
    Ok(BufReader::new(source).lines().count())
}

#[pyfunction]
fn copy(mut from: PyBinaryRead, mut to: PyBinaryWrite) -> PyResult<u64> {
    let copied = std::io::copy(&mut from, &mut to)?;
    to.flush()?;
    Ok(copied)
}
```

### Text streams count characters

Text wrappers deliberately don't implement `std::io`. Python text is characters, not bytes, so
they get their own small API:

```rust,no_run
use pyo3::prelude::*;
use pyo3_io::{PyTextRead, PyTextReadSeek, PyTextWrite};

#[pyfunction]
fn shout(mut source: PyTextRead, mut out: PyTextWrite) -> PyResult<()> {
    let text = source.read_to_string()?;
    out.write_all_str(&text.to_uppercase())?;
    Ok(())
}

#[pyfunction]
fn peek(mut source: PyTextReadSeek) -> PyResult<String> {
    let start = source.tell()?;           // an opaque position, like Python's
    let head = source.read_chars(80)?;    // at most 80 characters, not bytes
    source.seek_to(&start)?;
    Ok(head)
}
```

Need a specific encoding? Decide it in Python, where it belongs:

```python
peek(open("legacy.txt", encoding="latin-1"))
peek(io.TextIOWrapper(raw_stream, encoding="utf-16"))
```

### Detached or bound: who holds the GIL

Every wrapper comes in two forms:

|  | Detached: `PyBinaryRead`, … | Bound: `.bind(py)` / `.into_bound(py)` |
|---|---|---|
| Holds | its own reference to the object | a `Python<'py>` token |
| Attaches to Python | by itself, on every call | never, you already are |
| Release the GIL, move to another thread | ✅ `Send + 'static` | ❌ tied to `'py` |
| 64 B reads over 1 MiB (see [benchmarks](pytests/README.md#measured-results)) | 419 µs | 373 µs (1.12× faster) |

**Detached** is what `#[pyfunction]` arguments give you, and it shines when Rust does the heavy
lifting. Release the GIL, and each read re-attaches only for as long as it takes:

```rust,no_run
use pyo3::prelude::*;
use pyo3_io::PyBinaryRead;
use std::io::Read;

#[pyfunction]
fn digest(py: Python<'_>, mut source: PyBinaryRead) -> PyResult<usize> {
    py.detach(move || {
        let mut buf = [0u8; 64 * 1024];
        let mut total = 0;
        loop {
            match source.read(&mut buf)? {
                0 => return Ok(total),
                n => total += n, // hash `buf[..n]` here, other Python threads keep running
            }
        }
    })
}
```

**Bound** pays off when you hold the GIL anyway, e.g. to build Python objects as you read. There is
no attaching per call, which adds up over many small reads:

```rust,no_run
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList};
use pyo3_io::PyBinaryRead;
use std::io::{ErrorKind, Read};

/// Splits a stream of `u32`-length-prefixed records into a list of `bytes`.
#[pyfunction]
fn records<'py>(py: Python<'py>, source: PyBinaryRead) -> PyResult<Bound<'py, PyList>> {
    let mut source = source.into_bound(py);
    let records = PyList::empty(py);
    let mut header = [0u8; 4];
    loop {
        match source.read_exact(&mut header) {
            Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Ok(records),
            result => result?,
        }
        let mut record = vec![0u8; u32::from_le_bytes(header) as usize];
        source.read_exact(&mut record)?;
        records.append(PyBytes::new(py, &record))?;
    }
}
```

### Typed Python stubs (experimental)

With the `experimental-inspect` feature, every argument is described by a precise `Protocol` in
PyO3's generated stubs, so type checkers catch a text file passed where bytes are expected:

```python
def checksum(source: SupportsBinaryRead) -> int: ...

class SupportsBinaryRead(Protocol):
    def read(self, size: int, /) -> ReadableBuffer | None: ...
```

Enabling the feature is not enough on its own yet: writing these protocols into a `.pyi` needs a
custom build of PyO3's `pyo3-introspection` stub generator with `attach_to_root` support.

## Development

See [pytests/README.md](pytests/README.md) for running the test suite, rustdoc checks and
benchmarks.
