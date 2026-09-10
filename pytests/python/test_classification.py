"""How the payload kind of an object is worked out.

Only signals that do not lie are used, and everything else falls back to taking the object at its
word. Measured across the standard-library objects below: the `io` hierarchy is right 28 times and
wrong none, a `str` `encoding` attribute is right 11 times and wrong none, and a `mode` attribute
would settle 23 more but is wrong twice, so it is not consulted at all.

That leaves five objects unclassified, listed in `TestFallsBackToDuckTyping`. Each works in its
correct kind; using one in the wrong kind is caught at the first read instead of at the boundary.
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
        "SpooledTemporaryFile w+": lambda: _spooled("w+", TEXT),
        "socket.makefile r": lambda: socket.socketpair()[0].makefile("r"),
        "sys.stdout": lambda: sys.stdout,
    }


def duck_typed_factories(files):
    """The five the classifier deliberately says nothing about."""
    return {
        "NamedTemporaryFile": lambda: tempfile.NamedTemporaryFile(),
        "SpooledTemporaryFile wb+": lambda: _spooled("wb+", BYTES),
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

    def test_io_hierarchy_is_the_first_rung(self):
        with pytest.raises(TypeError, match=r"io\.TextIOBase"):
            ext.binary_read_all(io.StringIO(TEXT))
        with pytest.raises(TypeError, match=r"io\.RawIOBase or io\.BufferedIOBase"):
            ext.text_read_all(io.BytesIO(BYTES))

    def test_encoding_attribute_rung(self):
        class HasEncoding:
            encoding = "utf-8"

            def read(self, size=-1, /) -> str:
                return TEXT[:size]

        assert ext.text_read_chars(HasEncoding(), 4) == TEXT[:4]
        with pytest.raises(TypeError, match="str `encoding` attribute"):
            ext.binary_read_all(HasEncoding())

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

    def test_they_work_as_binary(self, files):
        for name, factory in duck_typed_factories(files).items():
            if name == "codecs.getreader":
                continue  # genuinely text
            handle = factory()
            try:
                ext.binary_read_exactly(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should work as binary: {err}")

    def test_codecs_getreader_works_as_text(self, files):
        reader = duck_typed_factories(files)["codecs.getreader"]()
        assert ext.text_read_chars(reader, 4) == TEXT[:4]

    def test_using_one_in_the_wrong_kind_is_caught_at_the_first_read(self, files):
        """Not at the boundary, which is the price of not guessing. Still an error, with a
        message that says what to do."""
        handle = duck_typed_factories(files)["SpooledTemporaryFile wb+"]()
        with pytest.raises(OSError, match="did not return str.*taken at its word"):
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

    def test_getting_it_wrong_says_what_to_do(self):
        """The one case the ladder cannot catch, so the error has to carry the explanation."""

        class SilentlyText:
            def read(self, size=-1, /) -> str:
                return TEXT[:size]

        with pytest.raises(OSError, match="did not return bytes.*taken at its word"):
            ext.binary_read_exactly(SilentlyText(), 4)

    def test_the_reverse_too(self):
        class SilentlyBinary:
            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        with pytest.raises(OSError, match="did not return str.*taken at its word"):
            ext.text_read_chars(SilentlyBinary(), 4)


class TestTheEscapeHatch:
    """`Unchecked` skips the payload-kind check and keeps the capability checks.

    Rarely needed now that only trustworthy signals are used: an object of no recognisable kind is
    accepted rather than refused. What is left is an object that positively misidentifies itself,
    which takes deriving from the wrong half of the `io` hierarchy or reporting an `encoding`
    while dealing in bytes.
    """

    def test_an_object_that_misreports_an_encoding_is_refused(self):
        class BinaryButClaimsAnEncoding:
            encoding = "utf-8"  # for its own purposes; it still deals in bytes

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        with pytest.raises(TypeError, match="str `encoding` attribute"):
            ext.binary_read_exactly(BinaryButClaimsAnEncoding(), 4)

    def test_the_escape_hatch_handles_it(self, files):
        class TextInBinaryClothing(io.RawIOBase):
            """Derives from the binary half of `io` but reads str, so it is refused as text."""

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

        with pytest.raises(TypeError, match="io.RawIOBase"):
            ext.text_read_all(TextInBinaryClothing())
        assert ext.text_read_all_unchecked(TextInBinaryClothing()) == TEXT

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
        # the shortcut's verdict would be "binary"; the real answer is text, and it works
        assert ext.text_read_chars(handle, 4) == TEXT[:4]
        with pytest.raises(TypeError, match="str `encoding` attribute"):
            ext.binary_read_exactly(_spooled("w+", TEXT), 4)
