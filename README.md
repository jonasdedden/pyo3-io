# pyo3-file-typed

Python file-like objects whose **payload kind** and **capabilities** are part of their Rust type.

```rust,ignore
use pyo3_file_typed::{BinaryRead, TextReadSeek};

#[pyfunction]
fn decompress(stream: BinaryRead) -> PyResult<Vec<u8>> { .. }

#[pyfunction]
fn count_lines(source: TextReadSeek) -> PyResult<usize> { .. }
```

`decompress` cannot be handed a text handle, `count_lines` cannot be handed a byte stream, and
neither can be written to or seeked unless it said so. All three are compile errors in Rust, and
with `experimental-inspect` all three are type errors in the generated stubs as well:

```python
class SupportsBinaryRead(Protocol):
    def read(self, size: int, /) -> ReadableBuffer: ...

class SupportsTextReadSeek(Protocol):
    def read(self, size: int, /) -> str: ...
    def seek(self, offset: int, whence: int, /) -> int: ...
    def tell(self, /) -> int: ...

def decompress(stream: SupportsBinaryRead) -> bytes: ...
def count_lines(source: SupportsTextReadSeek) -> int: ...
```

Nothing about that requires the caller to write stub-related code. Enabling `experimental-inspect`
on `pyo3` and on this crate is the whole opt-in.

## Against the two existing crates

|  | `pyo3-file` | `pyo3-filelike` | this crate |
|---|---|---|---|
| payload kind is static | no | **yes**, two types | yes |
| kind decided by | `isinstance(obj, io.TextIOBase)` | `mode` attribute, binary if absent | the Rust type; the object is checked for *contradicting* it by a `read(0)` probe |
| duck-typed objects | accepted | accepted, then fail inside the extraction | accepted |
| minimal writer (only `write`) | rejected | accepted | accepted |
| override for a misidentified object | n/a | n/a | `Unchecked<..>`, keeps the annotation |
| text reachable through `io::Read` | yes | yes | **no** |
| a text handle round-trips the file's bytes | no | no | **n/a — refused** |
| text reads work at every length | no | **yes** | yes |
| character-counted text API | no | no | **yes** |
| text seeking | `io::Seek`, wrong | **absent**, correct | cookie API |
| capabilities in the type | no | no | **yes** |
| rejection is | n/a | a panic | a `TypeError` |
| `fileno` failure is | a panic | a panic | an `io::Error` |
| implements `FromPyObject` | yes | **no** | yes |
| type stub says | `Any` | `Any` | an exact `Protocol` |

`pyo3-filelike` is much the closer of the two, and the rows it wins are ones `pyo3-file` gets
badly wrong. What is left is that the split stops at the type name: `PyTextFile` still implements
`std::io::Read`, so a text object still reaches a bytes-oriented consumer re-encoded as UTF-8, and
the latin-1 file below arrives as 48 bytes it does not contain. Its kind check is also a heuristic
— `mode` must be present and contain `b`, and anything without a `mode` attribute is assumed
binary, so `io.StringIO` and every duck-typed `-> str` reader get through and fail later at the
extraction. `tests/comparison.rs` asserts the two type-level claims so this table cannot go stale.

## Why not `pyo3-file`

`pyo3-file` has one type, `PyFileLikeObject`, that adapts to whatever it is given. It decides
between bytes and characters at runtime with `isinstance(obj, io.TextIOBase)`, and exposes both
through `std::io::Read`/`Write` by transcoding text to UTF-8. Four things follow from that.

**The bytes are not the file's bytes.** `std::io::Read` yields bytes, so a text object has to be
re-encoded. For anything that is not already UTF-8, what Rust receives is not what is on disk:

| | on disk | via a `"rb"` handle | via a `latin-1` handle |
|---|---|---|---|
| `pyo3-file` | `e9e8fc` (24 bytes) | `e9e8fc` | `c3a9c3a8c3bc` (48 bytes) |
| this crate | `e9e8fc` (24 bytes) | `e9e8fc` | refused, with a pointer to `.buffer` |

**Sizes mean different things on the two sides.** Python counts a text `read(size)` in characters,
`std::io::Read` counts bytes. `pyo3-file` bridges that by asking for `buf.len() / 4` characters,
the worst-case UTF-8 expansion, and refusing buffers under 4 bytes. Three quarters of every buffer
goes unused, and whether a read works at all depends on how the buffer happened to be sized:
`read_to_string` on a `StringIO` fails for 66 of the first 100 lengths, and never succeeds on a
text file of any realistic size.

**Some failures are panics.** `AsRawFd` cannot return an error, so `fileno()` raising
`io.UnsupportedOperation` — which `io.BytesIO` does — panics. Writing non-UTF-8 bytes to a text
object panics too, from the `expect` in the UTF-8 decode.

**The type stub can only say `Any`.** The requirements are `bool` arguments to a method called
inside the function body, so nothing about them reaches the signature PyO3 sees.

None of this is a bug list so much as one decision playing out: making the payload kind a runtime
property means every layer above it has to guess.

## The model

Bytes at the bottom, text as a layer above — which is what both languages already do.
`TextIOWrapper` wraps `BufferedReader` wraps `FileIO`; Rust puts `BufReader` and UTF-8 validation
over a byte stream. So this crate does not convert between the two. It asks the caller for the
kind it needs, and Python's own machinery does the conversion if one is required:

| you have | you need | one call |
|---|---|---|
| text object | bytes | `obj.buffer` |
| binary object | text | `io.TextIOWrapper(obj, encoding=...)` |

That is better than transcoding here, because the caller names the encoding instead of this crate
assuming UTF-8. The error messages say so, and name the evidence they used:

```text
TypeError: expected a binary file-like object, got the text object TextIOWrapper (it is an
io.TextIOBase). Pass obj.buffer to get at the bytes underneath, or open the file in binary mode.
Decoding text and re-encoding it here would not round-trip the file's bytes.
```

### How the kind is worked out

By asking the object, not by guessing from its type.

```python
>>> open(path, "rb").read(0)
b''
>>> open(path).read(0)
''
```

`read(0)` returns `b""` from a byte stream and `""` from a text one, which names the payload
exactly. It consumes nothing, never moves the stream position, and returns immediately even on an
empty *blocking* socket, where `read(1)` would hang forever. Across forty standard-library
file-like objects it was right forty times out of forty and failed not once.

That is ground truth rather than a correlation, so nothing structural can beat it — including a
class that derives from `io.RawIOBase` and returns `str` anyway, which no `isinstance` check can
catch:

```python
class Sneaky(io.RawIOBase):
    def read(self, size=-1, /) -> str: ...

isinstance(Sneaky(), io.RawIOBase)   # True  -- a structural check says "binary"
Sneaky().read(0)                     # ''    -- the probe says "text", correctly
```

It also settles the case this crate was built for: a minimal duck-typed reader that implements
nothing but `read` is classified up front, with no attributes to inspect and no base class to
consult.

The `io` hierarchy is still checked, for the files that cannot be probed — a write-only or
seek-only file is never asked to read, because that is a side effect with nothing to gain. Both
binary bases are named there because `RawIOBase` and `BufferedIOBase` are siblings under `IOBase`,
not one inside the other, and both halves hold ordinary objects; the shortcut of testing `IOBase`
and not `TextIOBase` would call `tempfile.SpooledTemporaryFile(mode="w+")` binary when it reads
`str`.

```text
  structural only  [io]          28 classified,  7 duck typed, 0 wrong
  probe first      [read(0), io] 35 classified,  0 duck typed, 0 wrong
```

#### What the probe costs

One `read(0)` per extraction, and one assumption: that the object honours the size argument. An
object whose `read(0)` hands back data has broken its contract *and* that data is now gone, so it
is reported rather than silently dropped:

```text
OSError: read(0) returned 120 items instead of nothing, so this object does not honour the size
argument of read(). The probe that identifies binary from text streams has consumed that data and
cannot put it back. Fix read() to return at most `size` items, or extract the file as
Unchecked<..> to skip the probe.
```

A loud failure beats a silent gap in the stream. `Unchecked<..>` skips the probe for an object
that cannot be asked, and carries the same type stub protocol so it costs nothing in the
annotation.

An object that simply declines — no `read`, a raise, or a return that is neither text nor a buffer
— is not an error; the probe just has no answer and the structural check takes over.

#### Signals deliberately not used

A `mode` attribute would settle 23 of the forty and be wrong about two: `codecs.getreader(..)`
wraps a binary file and reports *its* `"rb"` while producing `str`, so `pyo3-filelike`, which has
`mode` as its only signal, gets that exactly backwards. The `encoding` and `errors` a text stream
reports are a proxy for `io.TextIOBase`, which the structural check already covers.

`typing.BinaryIO` and `typing.TextIO` are no use either: they are plain classes rather than
`runtime_checkable` protocols, `isinstance` against them is `False` for everything including a
real `open(p, "rb")`, and they demand the full 27-member `IO` surface that this crate's protocols
exist to avoid.

Writing has an equivalent probe — `write(b"")` succeeds on a byte stream and raises `TypeError` on
a text one, unambiguously across every writer tested — and it is deliberately not used. Unlike a
zero-length read, a zero-length *write* calls the object's `write`, which for a user-defined
object is arbitrary code: a per-call writer records an empty entry. Failing on the first real
write costs one call instead of two and has no side effect of its own.

`test_classification.py` runs every rung and every standard-library file-like object I could find
— `gzip`, `bz2`, `lzma`, `zipfile`, `tarfile`, `tempfile`, `socket.makefile`, `subprocess` pipes,
`codecs`, `os.fdopen`, `sys.stdout` — through both kinds.

Nothing an object says about itself reaches the type stubs. The protocols come from the Rust type
alone, so classification only decides whether a particular object is let through at runtime.
[`Unchecked`] skips it for an object that positively misidentifies itself, and carries the same
protocol so it costs nothing in the annotation:

```rust,ignore
#[pyfunction]
fn read_it(source: Unchecked<TextRead>) -> PyResult<String> { .. }
// still generates: def read_it(source: SupportsTextRead) -> str: ...
```

### Binary

`PyFile<Binary, ..>` implements `std::io::Read`, `Write` and `Seek`. `read(buf)` is one
`read(len(buf))` and the counts mean the same thing on both sides, so there is no minimum buffer
size and no expansion factor.

### Text

`PyFile<Text, ..>` deliberately does **not** implement `std::io::Read`: there is no byte count for
it to honour. It offers a character API instead — `read_chars`, `read_to_string`, `write_str`,
`write_all_str` — and characters cross the boundary as characters.

Seeking is `tell` and `seek_to` rather than `std::io::Seek`, because a Python text stream's
positions are opaque cookies from `tell()`, not byte offsets. `SeekFrom::Current(4)` has no
meaning for one, and Python raises if you try.

## The aliases

Spell the parameters out as `PyFile<Binary, true, false, false, true>` if you like, but the thirty
generated aliases read better:

```rust,ignore
BinaryRead  BinaryWrite  BinarySeek  BinaryFileno  BinaryReadWrite  BinaryReadSeek …
TextRead    TextWrite    TextSeek    TextFileno    TextReadWrite    TextReadSeek   …
```

The capability order is always read, write, seek, fileno.

## Performance

Making the kind static removes work rather than adding it. Measured with `pytests/bench.py`:

```text
                             typed   pyo3-file    filelike   vs file vs flike

binary: read a whole stream
  4 KiB                     1.7 us      4.3 us      4.9 us     2.47x    2.78x
  64 KiB                    3.8 us     11.2 us     11.7 us     2.91x    3.06x
  1024 KiB                 58.8 us    134.0 us    134.6 us     2.28x    2.29x
  8192 KiB                497.1 us   1286.4 us   1284.5 us     2.59x    2.58x

text: read n characters
  16384 chars               5.4 us      6.4 us         n/a     1.18x           filelike: bytes only
  262144 chars             64.8 us     88.2 us         n/a     1.36x           filelike: bytes only

text: read a whole stream
  64 KiB                   19.7 us         n/a     29.7 us              1.51x  pyo3-file: 4-byte guard
  1024 KiB                432.2 us         n/a    473.0 us              1.09x  pyo3-file: 4-byte guard
```

### Where the binary margin comes from

Not from the call count, which is the obvious guess and the wrong one. Counting the `read` calls
an object actually receives:

```text
             this crate   pyo3-file / pyo3-filelike
  4 KiB          2                  9
  64 KiB         2                 13
  1024 KiB       2                 17
  8192 KiB       2                 20
```

Twenty crossings instead of two is worth perhaps 20 µs, and the 8 MiB gap is 786 µs. The real
cost is what those calls *ask for*. Both neighbours use `std`'s default `read_to_end`, which
doubles its buffer each time:

```text
  32, 32, 64, 128, 256, 512, 1024, 2048, ... 524288, 1048576
```

Summed, that is **exactly twice the size of the stream**, at every size measured. Every one of
those is a fresh Python `bytes` object allocated, filled and thrown away, and the `Vec` regrows
underneath for roughly another 2× of copying. This crate asks `read(-1)` once, so Python allocates
the stream once and it is copied once into a `Vec` that reaches its final size in a single
`reserve`.

So the margin is allocator and memcpy traffic, not FFI overhead — which is why it holds steady at
around 2.5x from 4 KiB to 8 MiB instead of shrinking as the calls amortise.

`read(-1)` is only safe to reach for because the implementation knows it is talking to a byte
stream. Text reads hand a `str` straight across instead of encoding it to UTF-8 and validating it
back again — and against `pyo3-filelike` there is a further copy saved, since its `PyTextFile`
stages everything through an internal `Vec<u8>` and `drain`s the front of it after every read.
Neither is a micro-optimisation bolted on; both fall out of not having to guess.

There is no measurable cost to the payload-kind check: it runs once per extraction and is a
handful of `isinstance` calls against cached types.

The one thing not done yet is `readinto`, which would let Python fill the Rust buffer directly and
remove the remaining copy. It needs a capability of its own, since not every object has one.

### Two forms, the way `pyo3` does it

`pyo3`'s own types split into a marker plus the handle that has the methods: `pub struct
PyList(PyAny)` is bare, and everything useful lives in `PyListMethods<'py>`, implemented for
`Bound<'py, PyList>`. This crate follows that shape:

| | owns | `Send + 'static` | attaches |
|---|---|---|---|
| `PyFile<M, ..>` | `Py<PyAny>` | yes | once per operation |
| `BoundFile<'py, M, ..>` | `Bound<'py, PyAny>` | no | never; it has the token |

The implementations live on `BoundFile`, and `PyFile` gets the same traits by attaching once and
delegating. So a caller that already holds the GIL says so and nothing is taken implicitly:

```rust,ignore
#[pyfunction]
fn count(py: Python<'_>, file: BinaryRead) -> PyResult<usize> {
    let mut file = file.into_bound(py);   // BoundBinaryRead<'py>
    let mut sink = Vec::new();
    Ok(file.read_to_end(&mut sink)?)
}
```

Every alias has a `Bound` twin — `BoundBinaryRead<'py>`, `BoundTextReadSeek<'py>` — so the type is
nameable when it has to be.

It could not literally be `Bound<'py, PyFile>`: that needs `PyFile` to be a native type with a
CPython type object behind it, and "any object with a `read` method" has none. `BoundFile` is the
same idea with its own struct.

#### Why the detached form exists at all

Attaching costs 2.51 ns with the GIL already held, which is 0.62% of a 64-byte read through
`io::Read` and 0.07% of a 64 KiB one, so the saving is not the point. The point is that
`BoundFile` borrows the token, which makes it neither `Send` nor `'static`, and plenty of code
wants a reader that is both:

```rust,ignore
fn takes_owned_reader<R: Read + Send + 'static>(_: R) {}   // zip::ZipArchive::new,
                                                            // csv::Reader::from_reader, io::copy
std::thread::spawn(move || file.read_to_end(&mut sink))     // no token held at all
```

The thread case is the sharp one, and it works: the parent releases the GIL entirely and the child
acquires it for the duration of each read and lets it go again. `tests/api_surface.rs` asserts
both properties, `tests/ui/bound_file_is_not_send.rs` asserts that `BoundFile` deliberately has
neither, and `read_on_another_thread` in the test extension actually does it.

## Errors

Everything fails with [`Error`], a `thiserror` enum, and it is meaningful in both directions.
Towards Rust it carries a real [`io::ErrorKind`](std::io::ErrorKind) rather than flattening to
`Other`:

| variant | kind |
|---|---|
| `WouldBlock` | `WouldBlock` |
| `WrongPayload`, `OverlongRead`, `ImpossibleWriteCount` | `InvalidData` |
| `WroteNothing` | `WriteZero` |
| `OutOfRange`, `MissingMethod`, `WrongKind` | `InvalidInput` |
| `Python` | whatever `pyo3` maps the exception to |

Towards Python, `MissingMethod` and `WrongKind` become `TypeError`, `WouldBlock` becomes
`BlockingIOError`, and the rest go through `io::Error`.

### `None` is not a failure

CPython's io protocol uses `None` for "nothing could be done right now", which is a different
thing from `b""` for end of stream. It is what an unbuffered non-blocking stream returns before
data arrives, and the *same object* returns real data on a later call:

```python
>>> r, w = os.pipe(); os.set_blocking(r, False)
>>> f = open(r, "rb", buffering=0)
>>> f.read(10)          # None: nothing yet
>>> os.write(w, b"hello")
>>> f.read(10)
b'hello'
```

So `None` maps to `io::ErrorKind::WouldBlock` and reaches Python as `BlockingIOError`. That is the
same exception a *buffered* stream raises by itself for the same condition, and `pyo3` already
maps that exception back to `WouldBlock`, so both of Python's signalling styles land in one place
and a caller only has to handle one. `write` returning `None` is the same story: the raw stream
could not take a single byte, and a retry may.

Neither neighbour does this. `pyo3-file` has no `None` case on read, so the extraction failure
surfaces as a non-retryable error, and on write it raises a plain `io::Error::other`.
`pyo3-filelike` discards `write`'s return value entirely, so a `None` write — and, unrelated to
non-blocking, *any* partial write — is reported as a complete success:

```python
>>> writer = Partial()                       # accepts 3 bytes per call
>>> filelike_write(writer, b"0123456789")
10
>>> writer.got
b'012'                                       # seven bytes silently dropped
```

`test_nonblocking.py` covers all of it, including a real non-blocking pipe, and asserts what each
neighbour does today so the comparison fails rather than going stale.

## Type stubs

The protocols are exact. Because the payload kind is fixed by the Rust type there is no
`isinstance` dispatch for a structural protocol to disagree with, so a duck-typed object with the
right methods is accepted by the type checker and by the runtime alike — and one without them is
rejected by both.

`read` returns `_typeshed.ReadableBuffer` rather than `bytes` because that is what the extraction
actually accepts: `bytes`, `bytearray`, `memoryview` and `array` all work, and so does the
`buffer` attribute of a text stream, which is the escape hatch the error message recommends. (The
runtime will also take a `Sequence[int]`; the protocol does not, since nothing real returns a
`list` from `read`.)

`flush` is deliberately *not* in the writing protocols. A `Protocol` cannot mark a member
optional, and an object with no `flush` has nothing to flush, so the implementation calls it only
when it is there and requiring it would turn away every minimal writer that implements nothing
else. `tell` is required for text seeking, in contrast, because it is the only way to get a
position `seek_to` will accept and there is no sensible way to degrade without it.

### Why `flush` is not a capability

`std::io::Write::flush` is a *required* trait method and `std` has no `Flush` trait, so there is no
"flush half" of `Write` to implement conditionally: `impl Write` must provide a `flush` whatever
the object has. The choice is between refusing objects that lack one and calling it only when it
is there.

This crate calls it when it is there. An object with no `flush` either does not buffer, in which
case `Ok(())` is truthful, or buffers with no way to be flushed, in which case nothing would help
— and every `io.IOBase` subclass inherits a `flush`, so the latter takes a duck-typed object that
buffers privately, which is broken by Python's own conventions.

Making flushability *demandable* is possible but expensive: a fifth capability, `io::Write`
implemented only for the flushable half, an inherent write API for the other half so it is not
left with nothing, and 46 protocols instead of 30. A plain `BinaryWrite` would lose `BufWriter`,
`write!` and `io::copy`. The gain is being able to say "this stream must be flushable" in a type,
which is worth less than that costs. `tests/api_surface.rs` records the decision.

Thirty protocols are linked into every extension, since they are `#[used]` statics. The stub
generator keeps the ones an annotation refers to and drops the rest.

This needs the `attach_to_root` support in `pyo3-introspection` (branch
`introspection-attach-to-root` of the neighbouring `pyo3` checkout): a library crate cannot know
the introspection id of the root module of whichever extension links it, so it cannot say where
its classes belong. The `pyo3` crate itself needs no change.

## Tests

`./pytests/run-tests.sh` runs all of it:

- **12 compile-fail tests** (`tests/ui/`) — every capability the type does not carry is a Rust
  compile error, including `io::Read` on a text file, which is the guarantee a gzip decoder wants.
- **6 error tests** (`tests/errors.rs`) — every variant reaches Rust with the right
  `io::ErrorKind` and Python with the right exception.
- **9 API surface tests** (`tests/api_surface.rs`) — the other half: everything that should exist
  does, for all thirty aliases.
- **3 comparison tests** (`tests/comparison.rs`) — the type-level claims the table above makes
  about `pyo3-filelike`, so it fails rather than going quietly out of date.
- **195 runtime tests** (`pytests/python/`) — payload round-tripping, character-vs-byte counting,
  the classification ladder against every standard-library file-like object, capability checks,
  misbehaving objects, and two files of side-by-side comparisons asserting both what `pyo3-file`
  and `pyo3-filelike` do today and what this crate does instead.
- **Type-checker tests** (`pytests/typecheck/`) — `pyright` over a file that must produce no
  errors and one that must produce exactly ten, one per rule.
- **A stub snapshot** — the generated `.pyi` is checked in, so any change to it shows up in review.
