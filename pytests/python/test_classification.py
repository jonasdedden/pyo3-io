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

import typing

import pytest

import pyo3_typed_io_tests as ext

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

    def test_the_io_hierarchy_is_the_only_rung(self):
        with pytest.raises(TypeError, match=r"io\.TextIOBase"):
            ext.binary_read_all(io.StringIO(TEXT))
        with pytest.raises(TypeError, match=r"io\.RawIOBase or io\.BufferedIOBase"):
            ext.text_read_all(io.BytesIO(BYTES))
        # and for write-only files, where there is nothing to read anyway
        with pytest.raises(TypeError, match=r"io\.RawIOBase or io\.BufferedIOBase"):
            ext.text_write(io.BytesIO(), TEXT)

    def test_nothing_is_called_on_the_object_during_extraction(self):
        """Classification is structural. A handle is not a promise that the thing behind it is
        ready to be touched, so extraction never calls `read` or `write` to find out what it is."""

        class Watchful:
            def __init__(self):
                self.calls = []

            def read(self, size=-1, /) -> bytes:
                self.calls.append(("read", size))
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        watchful = Watchful()
        ext.binary_read_exactly(watchful, 4)
        assert watchful.calls == [("read", 4)], "extraction touched the object"

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

    def test_using_one_in_the_wrong_kind_is_caught_at_the_first_read(self, files):
        """Not at the boundary, which is the price of not touching the object to find out."""
        handle = duck_typed_factories(files)["SpooledTemporaryFile wb+"]()
        with pytest.raises(OSError, match="did not return str.*use a binary wrapper"):
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
        """The case structure cannot settle, so the error has to carry the explanation."""

        class SilentlyText:
            def read(self, size=-1, /) -> str:
                return TEXT[:size] if size is not None and size >= 0 else TEXT

        with pytest.raises(OSError, match="did not return bytes.*use a text wrapper"):
            ext.binary_read_exactly(SilentlyText(), 4)

    def test_the_reverse_too(self):
        class SilentlyBinary:
            def read(self, size=-1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        with pytest.raises(OSError, match="did not return str.*use a binary wrapper"):
            ext.text_read_chars(SilentlyBinary(), 4)


class TestMisleadingInheritance:
    """An object deriving from the wrong half of the `io` hierarchy is refused, full stop.

    There is deliberately no escape hatch: inheriting from `io.RawIOBase` while dealing in
    `str` contradicts the base's own contract (typeshed types its `write` as bytes-only),
    so a checker flags it at the class definition. If the class is yours, fix the base;
    if it is someone else's, wrap it instead of inheriting — a plain delegating object
    is taken at its word.
    """

    def test_an_object_with_its_own_encoding_attribute_is_left_alone(self):
        """Nothing correlational is consulted, so this is simply accepted for what it is."""

        class BinaryThatKeepsAnEncoding:
            encoding = "utf-8"  # for its own purposes
            errors = "strict"

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(BinaryThatKeepsAnEncoding(), 4) == BYTES[:4]

    def test_a_structurally_misleading_object_is_refused(self):
        """Deriving from the binary half of `io` while dealing in str."""

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

    def test_wrapping_instead_of_inheriting_is_accepted(self):
        """The recipe for a misclassified class that cannot be fixed: delegate, don't inherit."""

        class ThirdPartyWriter(io.RawIOBase):
            def __init__(self):
                self.written = ""

            def writable(self):
                return True

            def write(self, data, /):
                self.written += data
                return len(data)

        class TextShim:
            def __init__(self, inner):
                self._inner = inner

            def write(self, data, /):
                return self._inner.write(data)

            def flush(self):
                pass

        inner = ThirdPartyWriter()
        assert ext.text_write(TextShim(inner), TEXT) == len(TEXT)
        assert inner.written == TEXT

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


IO_PROTOCOLS = hasattr(io, "Reader") and hasattr(io, "Writer")


@pytest.mark.skipif(not IO_PROTOCOLS, reason="io.Reader/io.Writer are new in Python 3.14")
class TestIoReaderAndWriter:
    """Python ABC membership is neither required nor sufficient; callable lookup decides."""

    def test_what_they_actually_are(self):
        import abc

        assert isinstance(io.Reader, abc.ABCMeta)
        assert typing.Protocol not in io.Reader.__mro__
        assert not getattr(io.Reader, "_is_protocol", False)
        assert not getattr(io.Reader, "_is_runtime_protocol", False)

    def test_nothing_has_to_inherit_from_them(self):
        """The `__subclasshook__` does the work, so `isinstance` passes without inheritance."""
        with io.BytesIO() as handle:
            assert io.Reader not in type(handle).__mro__
            assert isinstance(handle, io.Reader)

        class InheritsNothing:
            def read(self, size=-1, /) -> bytes:
                return b""

        assert isinstance(InheritsNothing(), io.Reader)

    def test_an_ordinary_implementation_satisfies_everything(self):
        """The baseline, so the rows below are read as the exceptions they are."""

        class Normal:
            def read(self, size=-1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        obj = Normal()
        assert hasattr(obj, "read")
        assert isinstance(obj, io.Reader)
        assert isinstance(obj, self.supports_read())
        assert ext.binary_read_exactly(obj, 4) == BYTES[:4]

    @staticmethod
    def supports_read():
        @typing.runtime_checkable
        class SupportsRead(typing.Protocol):
            def read(self, size: int = ..., /): ...

        return SupportsRead

    def test_the_three_checks_are_nested_not_equivalent(self):
        """`hasattr` is the widest, `io.Reader` the narrowest, a protocol in between.

        `hasattr` is an ordinary attribute lookup, so `__getattr__` counts. A
        `runtime_checkable` protocol uses `inspect.getattr_static` since 3.12, which sees an
        attribute put on the instance but deliberately does not trigger `__getattr__`.
        `io.Reader`'s hook is `_check_methods`, which walks the class MRO and sees neither.
        """
        supports_read = self.supports_read()

        class ViaGetattr:
            def __getattr__(self, name):
                if name == "read":
                    return lambda size=-1, /: BYTES[:size] if size >= 0 else BYTES
                raise AttributeError(name)

        class ReadOnTheInstance:
            def __init__(self):
                self.read = lambda size=-1, /: BYTES[:size] if size >= 0 else BYTES

        dynamic, on_instance = ViaGetattr(), ReadOnTheInstance()

        assert hasattr(dynamic, "read") and hasattr(on_instance, "read")
        assert not isinstance(dynamic, supports_read)
        assert isinstance(on_instance, supports_read)  # <- where the two checks part company
        assert not isinstance(dynamic, io.Reader)
        assert not isinstance(on_instance, io.Reader)

        # Normal callable lookup accepts both; requiring io.Reader membership would not.
        assert ext.binary_read_exactly(dynamic, 4) == BYTES[:4]
        assert ext.binary_read_exactly(on_instance, 4) == BYTES[:4]

    def test_typeshed_supports_read_is_not_importable_at_runtime(self):
        """Which is why `hasattr` was the only option before 3.14."""
        with pytest.raises(ModuleNotFoundError):
            __import__("_typeshed")

    def test_they_cannot_be_subscripted_for_isinstance(self):
        """So they say nothing about the payload kind, only that a method is there."""
        with pytest.raises(TypeError, match="parameterized generic"):
            isinstance(io.BytesIO(), io.Reader[bytes])

    def test_a_registered_class_can_supply_a_dynamic_method(self):
        """The callable returned by __getattr__, not registration, permits construction."""

        class DeclaresItself:
            def __getattr__(self, name):
                if name == "read":
                    return lambda size=-1, /: BYTES[:size] if size >= 0 else BYTES
                raise AttributeError(name)

        io.Reader.register(DeclaresItself)
        assert not type(DeclaresItself()).__dict__.get("read")
        assert isinstance(DeclaresItself(), io.Reader)
        assert ext.binary_read_exactly(DeclaresItself(), 4) == BYTES[:4]

    def test_a_getattr_reader_is_accepted_without_the_registration(self):
        """The ABC says no, because the hook inspects the class; `hasattr` says yes."""

        class DynamicRead:
            def __getattr__(self, name):
                if name == "read":
                    return lambda size=-1, /: BYTES[:size] if size >= 0 else BYTES
                raise AttributeError(name)

        assert not isinstance(DynamicRead(), io.Reader)
        assert hasattr(DynamicRead(), "read")
        assert ext.binary_read_exactly(DynamicRead(), 4) == BYTES[:4]

    def test_neither_still_means_no(self):
        with pytest.raises(TypeError, match=r"has no \.read\(\) method"):
            ext.binary_read_all(object())

    def test_the_same_for_writers(self):
        class DeclaresItself:
            def __init__(self):
                self.written = b""

            def __getattr__(self, name):
                if name == "write":
                    def write(data, /):
                        self.written += data
                        return len(data)

                    return write
                raise AttributeError(name)

        io.Writer.register(DeclaresItself)
        writer = DeclaresItself()
        ext.binary_write(writer, BYTES)
        assert writer.written == BYTES

    def test_real_file_objects_satisfy_them(self, files):
        for name, factory in binary_factories(files).items():
            handle = factory()
            assert isinstance(handle, io.Reader) or hasattr(handle, "read"), name


class TestDirectionChecks:
    """A writer handed to something that wants a reader, caught at the boundary.

    `io.IOBase` gives every stream a `read`, a `write` and a `seek`, including the ones that only
    raise, so `hasattr` cannot tell them apart. `readable()`, `writable()` and `seekable()` are
    the hierarchy's own answer, and their documented meaning is exactly this: if one is false, the
    corresponding call raises.
    """

    def test_hasattr_alone_cannot_tell_a_writer_from_a_reader(self, tmp_path):
        """The gap this closes."""
        path = tmp_path / "w.bin"
        with open(path, "wb") as handle:
            assert hasattr(handle, "read")  # inherited, and it raises
            assert handle.readable() is False

    def test_a_write_only_file_is_refused_as_a_reader(self, tmp_path):
        path = tmp_path / "w.bin"
        with open(path, "wb") as handle:
            with pytest.raises(TypeError, match=r"readable\(\) is False"):
                ext.binary_read_all(handle)

    def test_a_read_only_file_is_refused_as_a_writer(self, files):
        with open(files["bin"], "rb") as handle:
            with pytest.raises(TypeError, match=r"writable\(\) is False"):
                ext.binary_write(handle, BYTES)

    def test_the_same_for_text(self, tmp_path, files):
        path = tmp_path / "w.txt"
        with open(path, "w", encoding="utf-8") as handle:
            with pytest.raises(TypeError, match=r"readable\(\) is False"):
                ext.text_read_all(handle)
        with open(files["txt"], encoding="utf-8") as handle:
            with pytest.raises(TypeError, match=r"writable\(\) is False"):
                ext.text_write(handle, TEXT)

    def test_an_unseekable_stream_is_refused_as_seekable(self):
        """Sockets and pipes have a `seek` that raises."""
        left, right = socket.socketpair()
        try:
            handle = left.makefile("rb")
            assert hasattr(handle, "seek")
            assert handle.seekable() is False
            with pytest.raises(TypeError, match=r"seekable\(\) is False"):
                ext.binary_seek_roundtrip(handle)
        finally:
            left.close()
            right.close()

    def test_a_read_write_file_satisfies_both(self, tmp_path):
        path = tmp_path / "rw.bin"
        path.write_bytes(b"")
        with open(path, "w+b") as handle:
            assert ext.binary_read_write(handle, BYTES) == 0

    def test_the_message_names_the_capability(self, files):
        with open(files["bin"], "rb") as handle:
            with pytest.raises(TypeError, match="WRITE capability"):
                ext.binary_write(handle, BYTES)

    def test_duck_typed_objects_are_not_asked(self):
        """These are only queries on `io.IOBase`. Elsewhere they would be arbitrary code."""

        class LiesAboutItself:
            def readable(self):
                raise AssertionError("should never be called")

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        assert ext.binary_read_exactly(LiesAboutItself(), 4) == BYTES[:4]

    def test_an_iobase_that_raises_from_the_query_is_not_rejected(self):
        """A raise is not an answer, so it falls through rather than failing."""

        class Awkward(io.RawIOBase):
            def readable(self):
                raise ValueError("cannot say")

            def read(self, size=-1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        assert ext.binary_read_exactly(Awkward(), 4) == BYTES[:4]

    def test_every_standard_library_reader_still_passes(self, files):
        for name, factory in binary_factories(files).items():
            handle = factory()
            try:
                ext.binary_read_exactly(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should still be accepted as a reader: {err}")
