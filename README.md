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
assuming UTF-8. The error messages say so:

```text
TypeError: expected a binary file-like object, got the text object TextIOWrapper. Pass obj.buffer
to get at the bytes underneath, or open the file in binary mode. Decoding text and re-encoding it
here would not round-trip the file's bytes.
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
                                        typed    pyo3-file

binary: read a whole stream
  4 KiB                                 1.7 us       4.5 us   2.67x faster
  64 KiB                                4.1 us      11.8 us   2.89x faster
  1024 KiB                             64.2 us     139.9 us   2.18x faster
  8192 KiB                            522.8 us    1359.2 us   2.60x faster

text: read n characters
  16384 chars                           6.3 us       7.3 us   1.15x faster
  262144 chars                         74.6 us     107.1 us   1.44x faster

text: read a whole stream
  1024 KiB                            447.3 us         n/a    pyo3-file: buffer size must be at
                                                              least 4 bytes
```

Binary `read_to_end` is one `read(-1)` rather than a call per buffer-sized chunk, which is only
safe to do because the implementation knows it is talking to a byte stream. Text reads hand a
`str` straight across instead of encoding it to UTF-8 and validating it back again. Neither is a
micro-optimisation bolted on; both fall out of not having to guess.

## Type stubs

The protocols are exact. Because the payload kind is fixed by the Rust type there is no
`isinstance` dispatch for a structural protocol to disagree with, so a duck-typed object with the
right methods is accepted by the type checker and by the runtime alike — and one without them is
rejected by both.

`read` returns `_typeshed.ReadableBuffer` rather than `bytes` because that is what the extraction
actually accepts: `bytes`, `bytearray`, `memoryview` and `array` all work, and so does the
`buffer` attribute of a text stream, which is the escape hatch the error message recommends.
Everything that writes also requires `flush`, because a writable `PyFile` is a `std::io::Write`
and its `flush` calls the object's.

Thirty protocols are linked into every extension, since they are `#[used]` statics. The stub
generator keeps the ones an annotation refers to and drops the rest.

This needs the `attach_to_root` support in `pyo3-introspection` (branch
`introspection-attach-to-root` of the neighbouring `pyo3` checkout): a library crate cannot know
the introspection id of the root module of whichever extension links it, so it cannot say where
its classes belong. The `pyo3` crate itself needs no change.

## Tests

`./pytests/run-tests.sh` runs all of it:

- **11 compile-fail tests** (`tests/ui/`) — every capability the type does not carry is a Rust
  compile error, including `io::Read` on a text file, which is the guarantee a gzip decoder wants.
- **5 API surface tests** (`tests/api_surface.rs`) — the other half: everything that should exist
  does, for all thirty aliases.
- **117 runtime tests** (`pytests/python/`) — payload round-tripping, character-vs-byte counting,
  capability and payload-kind checks, misbehaving objects, and a file of side-by-side comparisons
  asserting both what `pyo3-file` does today and what this crate does instead.
- **Type-checker tests** (`pytests/typecheck/`) — `pyright` over a file that must produce no
  errors and one that must produce exactly eleven, one per rule.
- **A stub snapshot** — the generated `.pyi` is checked in, so any change to it shows up in review.
