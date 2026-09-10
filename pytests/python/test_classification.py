"""How the payload kind of an object is worked out.

One signal is used: the `io` hierarchy, which *defines* the payload kind rather than correlating
with it. Everything else falls back to taking the object at its word.

Measured across the standard-library objects below, the `io` hierarchy is right 28 times and wrong
none. A `mode` attribute would settle 23 more and be wrong about two. The `encoding`/`errors` a
text stream reports would settle two more, but only ever for something textual that is not an
`io.TextIOBase`, which the fallback accepts anyway — so it bought nothing but an earlier error,
at the price of turning away a binary object that keeps an `encoding` for its own reasons.

That leaves seven objects unclassified, listed in `TestFallsBackToDuckTyping`. Each works in its
correct kind; using one in the wrong kind is caught at the first read instead of at the boundary,
and there is no path on which it silently succeeds.
"""

import bz2
import codecs
import gzip
import io
import lzma
import os
import socket
import subprocess
import sys
import tarfile
import tempfile
import zipfile

import pytest

import pyo3_file_typed_tests as ext

BYTES = b"hello world " * 10
TEXT = "hello world " * 10


@pytest.fixture(scope="module")
def files(tmp_path_factory):
    root = tmp_path_factory.mktemp("cls")
    paths = {
        "bin": root / "a.bin",
        "txt": root / "a.txt",
        "gz": root / "a.gz",
        "zip": root / "a.zip",
        "tar": root / "a.tar",
    }
    paths["bin"].write_bytes(BYTES)
    paths["txt"].write_text(TEXT)
    with gzip.open(paths["gz"], "wb") as handle:
        handle.write(BYTES)
    with zipfile.ZipFile(paths["zip"], "w") as archive:
        archive.writestr("a.bin", BYTES)
    with tarfile.open(paths["tar"], "w") as archive:
        archive.add(paths["bin"], "a.bin")
    return paths


def binary_factories(files):
    return {
        "open rb": lambda: open(files["bin"], "rb"),
        "open rb unbuffered": lambda: open(files["bin"], "rb", buffering=0),
        "open r+b": lambda: open(files["bin"], "r+b"),
        "io.BytesIO": lambda: io.BytesIO(BYTES),
        "io.BufferedRWPair": lambda: io.BufferedRWPair(io.BytesIO(BYTES), io.BytesIO()),
        "gzip rb": lambda: gzip.open(files["gz"], "rb"),
        "gzip.GzipFile": lambda: gzip.GzipFile(files["gz"]),
        "bz2.BZ2File": lambda: bz2.BZ2File(io.BytesIO(bz2.compress(BYTES))),
        "lzma.LZMAFile": lambda: lzma.LZMAFile(io.BytesIO(lzma.compress(BYTES))),
        "zipfile.ZipExtFile": lambda: zipfile.ZipFile(files["zip"]).open("a.bin"),
        "tarfile ExFileObject": lambda: tarfile.open(files["tar"]).extractfile("a.bin"),
        "tempfile.TemporaryFile": lambda: tempfile.TemporaryFile(),
        "socket.makefile rb": lambda: socket.socketpair()[0].makefile("rb"),
        "os.fdopen rb": lambda: os.fdopen(os.open(files["bin"], os.O_RDONLY), "rb"),
        "subprocess pipe": lambda: subprocess.run(
            ["cat", str(files["bin"])], stdout=subprocess.PIPE, check=True
        )
        and io.BytesIO(BYTES),
        "sys.stdout.buffer": lambda: sys.stdout.buffer,
    }


def text_factories(files):
    return {
        "open r": lambda: open(files["txt"]),
        "io.StringIO": lambda: io.StringIO(TEXT),
        "io.TextIOWrapper": lambda: io.TextIOWrapper(io.BytesIO(BYTES)),
        "gzip rt": lambda: gzip.open(files["gz"], "rt"),
        "tempfile.TemporaryFile w+": lambda: tempfile.TemporaryFile("w+"),
        "socket.makefile r": lambda: socket.socketpair()[0].makefile("r"),
        "sys.stdout": lambda: sys.stdout,
    }


def duck_typed_factories(files):
    """The five the classifier deliberately says nothing about."""
    return {
        "NamedTemporaryFile": lambda: tempfile.NamedTemporaryFile(),
        "SpooledTemporaryFile wb+": lambda: _spooled("wb+", BYTES),
        "SpooledTemporaryFile w+": lambda: _spooled("w+", TEXT),
        "codecs.getreader": lambda: codecs.getreader("utf-8")(open(files["bin"], "rb")),
        "codecs.EncodedFile": lambda: codecs.EncodedFile(open(files["bin"], "rb"), "utf-8"),
    }


def _spooled(mode, payload):
    handle = tempfile.SpooledTemporaryFile(mode=mode)
    handle.write(payload)
    handle.seek(0)
    return handle


class TestStandardLibraryObjectsAreClassifiedCorrectly:
    """Every one of these is accepted by the matching kind and refused by the other."""

    def test_binary_objects_accepted_as_binary(self, files):
        for name, factory in binary_factories(files).items():
            handle = factory()
            try:
                ext.binary_read_exactly(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should be accepted as binary: {err}")

    def test_binary_objects_refused_as_text(self, files):
        for name, factory in binary_factories(files).items():
            if name == "sys.stdout.buffer":
                continue  # writable only; the read would fail for an unrelated reason
            with pytest.raises(TypeError, match="expected a text file-like object"):
                ext.text_read_chars(factory(), 4)
                pytest.fail(f"{name} should be refused as text")

    def test_text_objects_accepted_as_text(self, files):
        for name, factory in text_factories(files).items():
            if name == "sys.stdout":
                continue  # writable only
            handle = factory()
            try:
                ext.text_read_chars(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should be accepted as text: {err}")

    def test_text_objects_refused_as_binary(self, files):
        for name, factory in text_factories(files).items():
            with pytest.raises(TypeError, match="expected a binary file-like object"):
                ext.binary_read_exactly(factory(), 4)
                pytest.fail(f"{name} should be refused as binary")


class TestTheLadderRungs:
    """The two rungs that are used, and the one that is not."""

    def test_the_probe_is_the_first_rung(self):
        """It reads nothing and names the type, so nothing structural can beat it."""
        with pytest.raises(TypeError, match=r"read\(0\) returned str"):
            ext.binary_read_all(io.StringIO(TEXT))
        with pytest.raises(TypeError, match=r"read\(0\) returned bytes"):
            ext.text_read_all(io.BytesIO(BYTES))

    def test_the_io_hierarchy_covers_what_cannot_be_probed(self):
        """A write-only file is never asked to read, so the structural check still earns its place."""
        with pytest.raises(TypeError, match=r"io\.RawIOBase or io\.BufferedIOBase"):
            ext.text_write(io.BytesIO(), TEXT)
        with pytest.raises(TypeError, match=r"io\.TextIOBase"):
            ext.binary_write(io.StringIO(), BYTES)

    def test_the_probe_beats_a_class_that_lies_structurally(self):
        """Deriving from the binary half of `io` while returning str. No structural check can
        catch this; the probe simply asks."""

        class Sneaky(io.RawIOBase):
            def __init__(self):
                self.left = TEXT

            def readable(self):
                return True

            def read(self, size=-1, /) -> str:
                if size is None or size < 0:
                    chunk, self.left = self.left, ""
                else:
                    chunk, self.left = self.left[:size], self.left[size:]
                return chunk

        assert isinstance(Sneaky(), io.RawIOBase)  # the structural answer would be "binary"
        assert ext.text_read_all(Sneaky()) == TEXT  # the probe gets it right

    def test_mode_is_never_consulted(self):
        """It would settle 23 more objects, and be wrong about two of them.

        Both are `codecs` wrappers, which report the *wrapped* binary file's mode while producing
        str. Rather than special-casing the exception, the unreliable signal is simply not used:
        an object whose `mode` is the only thing it says about itself is duck typed.
        """

        class ModeSaysBinaryButReadsText:
            mode = "rb"

            def read(self, size=-1, /) -> str:
                return TEXT[:size]

        class ModeSaysTextButReadsBytes:
            mode = "r"

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        # Both are accepted for what they actually are, because `mode` is not believed either way.
        assert ext.text_read_chars(ModeSaysBinaryButReadsText(), 4) == TEXT[:4]
        assert ext.binary_read_exactly(ModeSaysTextButReadsBytes(), 4) == BYTES[:4]

    def test_the_real_codecs_readers_work_without_being_special_cased(self, files):
        """The case that made `mode` untrustworthy, handled by not trusting `mode`."""
        reader = codecs.getreader("utf-8")(open(files["bin"], "rb"))
        assert reader.mode == "rb"  # it does say this
        assert ext.text_read_chars(reader, 5) == TEXT[:5]  # and it is text anyway

    def test_an_encoding_property_that_raises_does_not_break_classification(self):
        class Awkward:
            @property
            def encoding(self):
                raise RuntimeError("boom")

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(Awkward(), 4) == BYTES[:4]

    def test_a_non_string_encoding_is_ignored(self):
        class NumericEncoding:
            encoding = 42

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(NumericEncoding(), 4) == BYTES[:4]


class TestFallsBackToDuckTyping:
    """The objects no trustworthy signal covers. Each works in its correct kind."""

    TEXTUAL = {"codecs.getreader", "SpooledTemporaryFile w+"}

    def test_they_work_as_binary(self, files):
        for name, factory in duck_typed_factories(files).items():
            if name in self.TEXTUAL:
                continue
            handle = factory()
            try:
                ext.binary_read_exactly(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should work as binary: {err}")

    def test_the_textual_ones_work_as_text(self, files):
        for name in self.TEXTUAL:
            handle = duck_typed_factories(files)[name]()
            assert ext.text_read_chars(handle, 4) == TEXT[:4], name

    def test_using_one_in_the_wrong_kind_is_caught_at_the_boundary(self, files):
        """The probe settles it at extraction, since these are all readable."""
        handle = duck_typed_factories(files)["SpooledTemporaryFile wb+"]()
        with pytest.raises(TypeError, match=r"read\(0\) returned bytes"):
            ext.text_read_chars(handle, 4)


class TestDuckTypingIsTheFallback:
    """An object that says nothing about itself is taken at its word, in either kind."""

    def test_bare_reader_works_as_binary(self):
        class Bare:
            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(Bare(), 4) == BYTES[:4]

    def test_bare_reader_works_as_text(self):
        class Bare:
            def read(self, size=-1, /) -> str:
                return TEXT[:size]

        assert ext.text_read_chars(Bare(), 4) == TEXT[:4]

    def test_a_writer_needs_nothing_but_write(self):
        """No `flush`: an object without one has nothing to flush."""

        class WriteOnly:
            def __init__(self) -> None:
                self.chunks: list[bytes] = []

            def write(self, data: bytes, /) -> int:
                self.chunks.append(data)
                return len(data)

        writer = WriteOnly()
        assert ext.binary_write(writer, BYTES) == len(BYTES)
        assert b"".join(writer.chunks) == BYTES

    def test_a_text_writer_needs_nothing_but_write(self):
        class WriteOnly:
            def __init__(self) -> None:
                self.chunks: list[str] = []

            def write(self, data: str, /) -> int:
                self.chunks.append(data)
                return len(data)

        writer = WriteOnly()
        assert ext.text_write(writer, TEXT) == len(TEXT)
        assert "".join(writer.chunks) == TEXT

    def test_flush_is_still_called_when_present(self):
        class Counting:
            def __init__(self) -> None:
                self.flushes = 0

            def write(self, data: bytes, /) -> int:
                return len(data)

            def flush(self) -> None:
                self.flushes += 1

        writer = Counting()
        ext.binary_write(writer, b"x")
        assert writer.flushes == 1

    def test_a_bare_reader_is_still_classified_by_the_probe(self):
        """The minimal case: a class with nothing but `read`, and it is still settled up front."""

        class OnlyReadsText:
            def read(self, size=-1, /) -> str:
                return TEXT[:size] if size is not None and size >= 0 else TEXT

        class OnlyReadsBytes:
            def read(self, size=-1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        with pytest.raises(TypeError, match=r"read\(0\) returned str"):
            ext.binary_read_exactly(OnlyReadsText(), 4)
        with pytest.raises(TypeError, match=r"read\(0\) returned bytes"):
            ext.text_read_chars(OnlyReadsBytes(), 4)

    def test_the_late_error_remains_for_what_cannot_be_probed(self):
        """An object that refuses to answer `read(0)` still gets the explanatory failure."""

        class WillNotBeProbed:
            def read(self, size=-1, /):
                if size == 0:
                    raise ValueError("I do not do zero-length reads")
                return TEXT[:size] if size is not None and size >= 0 else TEXT

        with pytest.raises(OSError, match="did not return bytes.*taken at its word"):
            ext.binary_read_exactly(WillNotBeProbed(), 4)


class TestTheEscapeHatch:
    """`Unchecked` skips the payload-kind check and keeps the capability checks.

    Rarely needed now that only trustworthy signals are used: an object of no recognisable kind is
    accepted rather than refused. What is left is an object that positively misidentifies itself,
    which takes deriving from the wrong half of the `io` hierarchy or reporting an `encoding`
    while dealing in bytes.
    """

    def test_an_object_with_its_own_encoding_attribute_is_left_alone(self):
        """Nothing correlational is consulted, so this is simply accepted for what it is."""

        class BinaryThatKeepsAnEncoding:
            encoding = "utf-8"  # for its own purposes
            errors = "strict"

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(BinaryThatKeepsAnEncoding(), 4) == BYTES[:4]

    def test_the_escape_hatch_handles_what_cannot_be_probed(self, files):
        """A write-only file is never probed, so a structurally misleading one still needs it."""

        class TextWriterInBinaryClothing(io.RawIOBase):
            def __init__(self):
                self.written = ""

            def writable(self):
                return True

            def write(self, data, /):
                self.written += data
                return len(data)

        with pytest.raises(TypeError, match="io.RawIOBase"):
            ext.text_write(TextWriterInBinaryClothing(), TEXT)

    def test_the_escape_hatch_still_checks_capabilities(self):
        with pytest.raises(TypeError, match=r"has no \.read\(\) method"):
            ext.text_read_all_unchecked(object())

    def test_the_escape_hatch_keeps_its_annotation(self, stub_source):
        """`Unchecked<TextRead>` carries the same protocol as `TextRead`, so reaching for it does
        not cost the signature."""
        assert "def text_read_all_unchecked(file: SupportsTextRead) -> str" in stub_source

    def test_no_signal_reaches_the_type_stubs(self, stub_source):
        """Classification is a runtime concern. The protocols come from the Rust type, so nothing
        an object claims about itself can influence what the type checker demands."""
        assert "mode" not in stub_source
        assert "encoding" not in stub_source


class TestWhyBothBinaryBasesAreNamed:
    """`RawIOBase` and `BufferedIOBase` are siblings, and the obvious shortcut is wrong.

    Neither is inside the other, nothing is both, and both halves hold ordinary objects. Checking
    `IOBase` and not `TextIOBase` instead would collapse them into one test -- and swallow the
    middle ground where `tempfile.SpooledTemporaryFile` lives.
    """

    def test_the_two_bases_are_siblings(self):
        assert io.RawIOBase not in io.BufferedIOBase.__mro__
        assert io.BufferedIOBase not in io.RawIOBase.__mro__

    def test_raw_only_objects_need_the_raw_check(self, tmp_path):
        """Unbuffered files are `RawIOBase` and nothing else."""
        path = tmp_path / "raw.bin"
        path.write_bytes(BYTES)
        with open(path, "rb", buffering=0) as handle:
            assert isinstance(handle, io.RawIOBase)
            assert not isinstance(handle, io.BufferedIOBase)
            assert ext.binary_read_exactly(handle, 4) == BYTES[:4]

    def test_buffered_only_objects_need_the_buffered_check(self):
        """Nearly everything else is `BufferedIOBase` and nothing else."""
        handle = io.BytesIO(BYTES)
        assert isinstance(handle, io.BufferedIOBase)
        assert not isinstance(handle, io.RawIOBase)
        assert ext.binary_read_exactly(handle, 4) == BYTES[:4]

    def test_nothing_is_both(self, files):
        for factory in list(binary_factories(files).values()):
            handle = factory()
            assert not (
                isinstance(handle, io.RawIOBase) and isinstance(handle, io.BufferedIOBase)
            ), handle

    def test_the_iobase_shortcut_would_break_this(self):
        """`SpooledTemporaryFile` is `IOBase` alone, and its text form reads `str`.

        `isinstance(obj, IOBase) and not isinstance(obj, TextIOBase)` would call this binary. The
        `io` rung runs first, so it would win over the `encoding` rung that gets it right today.
        """
        handle = _spooled("w+", TEXT)
        assert isinstance(handle, io.IOBase)
        assert not isinstance(handle, (io.RawIOBase, io.BufferedIOBase, io.TextIOBase))
        # The shortcut's verdict would be "binary", which is wrong. The real ladder gives no
        # verdict at all, so the object is taken at its word and works.
        assert ext.text_read_chars(handle, 4) == TEXT[:4]


class TestThePayloadProbe:
    """`read(0)` reads nothing and names the type. What it does and does not promise."""

    def test_it_consumes_nothing(self, files):
        """The position is where it was, so the whole stream is still there afterwards."""
        with open(files["bin"], "rb") as handle:
            assert ext.binary_read_all(handle) == BYTES
        with open(files["txt"], encoding="utf-8") as handle:
            assert ext.text_read_all(handle) == TEXT

    def test_it_does_not_block_on_an_empty_blocking_socket(self):
        """`read(1)` there would hang forever; a zero-length read never has to wait."""
        left, right = socket.socketpair()
        try:
            with pytest.raises(TypeError, match=r"read\(0\) returned bytes"):
                ext.text_read_chars(left.makefile("rb"), 4)
        finally:
            left.close()
            right.close()

    def test_an_object_that_refuses_to_be_probed_falls_through(self):
        class NoZeroReads:
            def read(self, size=-1, /) -> bytes:
                if size == 0:
                    raise ValueError("no")
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        assert ext.binary_read_exactly(NoZeroReads(), 4) == BYTES[:4]

    def test_an_object_returning_something_else_falls_through(self):
        class ReturnsAList:
            def read(self, size=-1, /):
                return [] if size == 0 else list(BYTES[:size])

        # Neither text nor a buffer, so the probe declines to answer and this is accepted.
        assert ext.binary_read_exactly(ReturnsAList(), 4) == BYTES[:4]

    def test_write_only_files_are_never_probed(self):
        """Asking a writer to read is a side effect with nothing to gain."""

        class WriterThatWouldNoticeARead:
            def __init__(self):
                self.reads = 0
                self.written = b""

            def read(self, size=-1, /) -> bytes:
                self.reads += 1
                return b""

            def write(self, data: bytes, /) -> int:
                self.written += data
                return len(data)

        writer = WriterThatWouldNoticeARead()
        ext.binary_write(writer, BYTES)
        assert writer.written == BYTES
        assert writer.reads == 0

    def test_an_object_that_ignores_the_size_is_reported_not_silently_drained(self):
        """`read(0)` handing back data means the object broke its contract and the data is gone.

        A loud failure beats a silent gap in the stream.
        """

        class IgnoresSize:
            def read(self, size=-1, /) -> bytes:
                return BYTES

        with pytest.raises(OSError, match="does not honour the size argument"):
            ext.binary_read_exactly(IgnoresSize(), 4)

    def test_and_unchecked_skips_the_probe_entirely(self):
        class IgnoresSize:
            def __init__(self):
                self.left = TEXT

            def read(self, size=-1, /) -> str:
                chunk, self.left = self.left, ""
                return chunk

        assert ext.text_read_all_unchecked(IgnoresSize()) == TEXT
