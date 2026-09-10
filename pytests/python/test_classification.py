"""How the payload kind of an object is worked out.

The ladder prefers explicit evidence and falls back to taking the object at its word, so that a
class implementing nothing but the methods it needs still works. Each rung is here, and so is
every standard-library file-like object I could find, because the cheap signals are the
unreliable ones and the ordering is the whole design.
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
        "SpooledTemporaryFile wb+": lambda: _spooled("wb+", BYTES),
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
        "codecs.getreader": lambda: codecs.getreader("utf-8")(open(files["bin"], "rb")),
        "sys.stdout": lambda: sys.stdout,
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
    """Each rung, and why it sits where it does."""

    def test_io_hierarchy_is_the_first_rung(self):
        with pytest.raises(TypeError, match=r"io\.TextIOBase"):
            ext.binary_read_all(io.StringIO(TEXT))
        with pytest.raises(TypeError, match=r"io\.RawIOBase or io\.BufferedIOBase"):
            ext.text_read_all(io.BytesIO(BYTES))

    def test_codecs_wrappers_are_recognised_before_mode(self, files):
        """`codecs.getreader` reports the *wrapped* file's mode of 'rb' while producing str.

        Anything that consulted `mode` first would call this binary and get it backwards, which is
        what `pyo3-filelike` does.
        """
        reader = codecs.getreader("utf-8")(open(files["bin"], "rb"))
        assert reader.mode == "rb"
        assert ext.text_read_chars(reader, 5) == TEXT[:5]

        with pytest.raises(TypeError, match="codecs stream wrapper"):
            ext.binary_read_all(codecs.getreader("utf-8")(open(files["bin"], "rb")))

    def test_encoding_attribute_rung(self):
        class HasEncoding:
            encoding = "utf-8"

            def read(self, size=-1, /) -> str:
                return TEXT[:size]

        assert ext.text_read_chars(HasEncoding(), 4) == TEXT[:4]
        with pytest.raises(TypeError, match="str `encoding` attribute"):
            ext.binary_read_all(HasEncoding())

    def test_mode_attribute_rung(self):
        """The last resort, and what `SpooledTemporaryFile` leaves to go on."""

        class ModeR:
            mode = "r"

            def read(self, size=-1, /) -> str:
                return TEXT[:size]

        class ModeRB:
            mode = "rb"

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.text_read_chars(ModeR(), 4) == TEXT[:4]
        assert ext.binary_read_exactly(ModeRB(), 4) == BYTES[:4]
        with pytest.raises(TypeError, match=r"`mode` attribute does not contain 'b'"):
            ext.binary_read_all(ModeR())
        with pytest.raises(TypeError, match=r"`mode` attribute contains 'b'"):
            ext.text_read_all(ModeRB())

    def test_a_property_that_raises_does_not_break_classification(self):
        class Awkward:
            @property
            def mode(self):
                raise RuntimeError("boom")

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(Awkward(), 4) == BYTES[:4]

    def test_a_non_string_mode_is_ignored(self):
        class NumericMode:
            mode = 0o644

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(NumericMode(), 4) == BYTES[:4]


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


class TestTheMoodOfTheModeAttribute:
    """`mode` is the least reliable rung, so what it can and cannot do is pinned here.

    A sweep of forty standard-library file-like objects found `mode` disagreeing with what `read`
    actually returns in exactly two of them, `codecs.getreader(..)` and `codecs.open(..)`, both of
    which report the *wrapped* file's mode. Both are caught a rung earlier, by name. What is left
    is a third-party wrapper doing the same thing without deriving from the `codecs` classes.
    """

    def test_the_residual_limitation_is_real(self):
        """A codecs-alike that is not a codecs subclass is classified by its `mode` and refused.

        This is the documented limitation. It is a false rejection rather than a corruption, and
        the message names `mode` as the reason so it is diagnosable.
        """

        class HomeGrownReader:
            """Wraps a binary file and decodes it, like codecs, but inherits from nothing."""

            def __init__(self, inner):
                self._inner = inner
                self.mode = inner.mode  # "rb", copied from the file underneath

            def read(self, size=-1, /) -> str:
                return self._inner.read(size).decode("utf-8")

        with open(__file__, "rb") as inner:
            with pytest.raises(TypeError, match=r"`mode` attribute contains 'b'"):
                ext.text_read_all(HomeGrownReader(inner))

    def test_the_escape_hatch_handles_it(self):
        """`py_new_unchecked` skips the payload-kind check and keeps the capability checks."""

        class HomeGrownReader:
            def __init__(self, inner):
                self._inner = inner
                self.mode = inner.mode

            def read(self, size=-1, /) -> str:
                return self._inner.read(size).decode("utf-8")

        with open(__file__, "rb") as inner:
            got = ext.text_read_all_unchecked(HomeGrownReader(inner))
        assert "the mood of the mode attribute" in got.lower()

    def test_the_escape_hatch_still_checks_capabilities(self):
        with pytest.raises(TypeError, match=r"has no \.read\(\) method"):
            ext.text_read_all_unchecked(object())

    def test_mode_never_reaches_the_type_stubs(self, stub_source):
        """It is a runtime signal only. The protocols come from the Rust type, so nothing an
        object says about itself can influence what the type checker demands."""
        assert "mode" not in stub_source
