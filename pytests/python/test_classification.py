"""How the payload kind of an object is worked out.

Only the `io` hierarchy is consulted; any other object is taken at its word, and using it as the
wrong kind fails at the first read.
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
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest

import pyo3_io_tests as ext

BYTES = b"hello world " * 10
TEXT = "hello world " * 10


@pytest.fixture(scope="module")
def files(tmp_path_factory: pytest.TempPathFactory) -> dict[str, Path]:
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


def _in_io_hierarchy(factories: dict[str, Callable[[], Any]]) -> dict[str, Callable[[], Any]]:
    """Outside POSIX, `TemporaryFile` is `NamedTemporaryFile`, whose wrapper is not in the `io`
    hierarchy; that object is covered by `duck_typed_factories` instead."""
    if tempfile.TemporaryFile is not tempfile.NamedTemporaryFile:
        return factories
    return {name: f for name, f in factories.items() if not name.startswith("tempfile.")}


def binary_factories(files: dict[str, Path]) -> dict[str, Callable[[], Any]]:
    return _in_io_hierarchy(
        {
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
            "subprocess pipe": lambda: (
                subprocess.run(["cat", str(files["bin"])], stdout=subprocess.PIPE, check=True)
                and io.BytesIO(BYTES)
            ),
            "sys.stdout.buffer": lambda: sys.stdout.buffer,
        }
    )


def text_factories(files: dict[str, Path]) -> dict[str, Callable[[], Any]]:
    return _in_io_hierarchy(
        {
            "open r": lambda: open(files["txt"]),
            "io.StringIO": lambda: io.StringIO(TEXT),
            "io.TextIOWrapper": lambda: io.TextIOWrapper(io.BytesIO(BYTES)),
            "gzip rt": lambda: gzip.open(files["gz"], "rt"),
            "tempfile.TemporaryFile w+": lambda: tempfile.TemporaryFile("w+"),
            "socket.makefile r": lambda: socket.socketpair()[0].makefile("r"),
            "sys.stdout": lambda: sys.stdout,
        }
    )


def duck_typed_factories(files: dict[str, Path]) -> dict[str, Callable[[], Any]]:
    """Standard-library objects outside the `io` subclasses that decide the kind."""
    return {
        "NamedTemporaryFile": lambda: tempfile.NamedTemporaryFile(),
        "SpooledTemporaryFile wb+": lambda: _spooled("wb+", BYTES),
        "SpooledTemporaryFile w+": lambda: _spooled("w+", TEXT),
        "codecs.getreader": lambda: codecs.getreader("utf-8")(open(files["bin"], "rb")),
        "codecs.EncodedFile": lambda: codecs.EncodedFile(open(files["bin"], "rb"), "utf-8"),
    }


def _spooled(mode: str, payload: Any) -> Any:
    handle: tempfile.SpooledTemporaryFile[Any] = tempfile.SpooledTemporaryFile(mode=mode)  # noqa: SIM115 - the caller closes it
    handle.write(payload)
    handle.seek(0)
    return handle


class TestStandardLibraryObjectsAreClassifiedCorrectly:
    """Every one of these is accepted by the matching kind and refused by the other."""

    def test_binary_objects_accepted_as_binary(self, files: dict[str, Path]) -> None:
        for name, factory in binary_factories(files).items():
            handle = factory()
            try:
                ext.binary_read_exactly(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should be accepted as binary: {err}")

    def test_binary_objects_refused_as_text(self, files: dict[str, Path]) -> None:
        for name, factory in binary_factories(files).items():
            if name == "sys.stdout.buffer":
                continue  # writable only; the read would fail for an unrelated reason
            with pytest.raises(TypeError, match="expected a text file-like object"):
                ext.text_read_chars(factory(), 4)
                pytest.fail(f"{name} should be refused as text")

    def test_text_objects_accepted_as_text(self, files: dict[str, Path]) -> None:
        for name, factory in text_factories(files).items():
            if name == "sys.stdout":
                continue  # writable only
            handle = factory()
            try:
                ext.text_read_chars(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should be accepted as text: {err}")

    def test_text_objects_refused_as_binary(self, files: dict[str, Path]) -> None:
        for name, factory in text_factories(files).items():
            with pytest.raises(TypeError, match="expected a binary file-like object"):
                ext.binary_read_exactly(factory(), 4)
                pytest.fail(f"{name} should be refused as binary")


class TestOnlyTheIoHierarchyIsConsulted:
    def test_the_error_names_the_io_base(self) -> None:
        with pytest.raises(TypeError, match=r"io\.TextIOBase"):
            # intentional wrong-kind rejection
            ext.binary_read_all(io.StringIO(TEXT))  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
        with pytest.raises(TypeError, match=r"io\.RawIOBase or io\.BufferedIOBase"):
            # intentional wrong-kind rejection
            ext.text_read_all(io.BytesIO(BYTES))  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
        # and for write-only files, where there is nothing to read anyway
        with pytest.raises(TypeError, match=r"io\.RawIOBase or io\.BufferedIOBase"):
            # intentional wrong-kind rejection
            ext.text_write(io.BytesIO(), TEXT)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_nothing_is_called_on_the_object_during_extraction(self) -> None:

        class Watchful:
            def __init__(self) -> None:
                self.calls: list[tuple[str, int | None]] = []

            def read(self, size: int | None = -1, /) -> bytes:
                self.calls.append(("read", size))
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        watchful = Watchful()
        ext.binary_read_exactly(watchful, 4)
        assert watchful.calls == [("read", 4)], "extraction touched the object"

    def test_mode_is_never_consulted(self) -> None:
        """`codecs` readers report their binary source's mode while returning `str`."""

        class ModeSaysBinaryButReadsText:
            mode = "rb"

            def read(self, size: int = -1, /) -> str:
                return TEXT[:size]

        class ModeSaysTextButReadsBytes:
            mode = "r"

            def read(self, size: int = -1, /) -> bytes:
                return BYTES[:size]

        assert ext.text_read_chars(ModeSaysBinaryButReadsText(), 4) == TEXT[:4]
        assert ext.binary_read_exactly(ModeSaysTextButReadsBytes(), 4) == BYTES[:4]

    def test_the_real_codecs_reader(self, files: dict[str, Path]) -> None:
        with open(files["bin"], "rb") as raw:
            reader = codecs.getreader("utf-8")(raw)
            assert reader.mode == "rb"
            assert ext.text_read_chars(reader, 5) == TEXT[:5]

    def test_encoding_is_never_consulted(self) -> None:
        class RaisingEncoding:
            @property
            def encoding(self) -> Any:
                raise RuntimeError("boom")

            def read(self, size: int = -1, /) -> bytes:
                return BYTES[:size]

        class BinaryWithEncoding:
            encoding = "utf-8"

            def read(self, size: int = -1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(RaisingEncoding(), 4) == BYTES[:4]
        assert ext.binary_read_exactly(BinaryWithEncoding(), 4) == BYTES[:4]


class TestUnclassifiedStandardLibraryObjects:
    """Each works in its correct kind."""

    TEXTUAL = frozenset({"codecs.getreader", "SpooledTemporaryFile w+"})

    def test_they_work_as_binary(self, files: dict[str, Path]) -> None:
        for name, factory in duck_typed_factories(files).items():
            if name in self.TEXTUAL:
                continue
            handle = factory()
            try:
                ext.binary_read_exactly(handle, 4)
            except Exception as err:  # noqa: BLE001
                pytest.fail(f"{name} should work as binary: {err}")

    def test_the_textual_ones_work_as_text(self, files: dict[str, Path]) -> None:
        for name in self.TEXTUAL:
            handle = duck_typed_factories(files)[name]()
            assert ext.text_read_chars(handle, 4) == TEXT[:4], name

    def test_using_one_in_the_wrong_kind_is_caught_at_the_first_read(
        self, files: dict[str, Path]
    ) -> None:
        handle = duck_typed_factories(files)["SpooledTemporaryFile wb+"]()
        with pytest.raises(OSError, match=r"did not return str.*use a binary wrapper"):
            ext.text_read_chars(handle, 4)


class TestDuckTypedObjects:
    def test_bare_reader_works_as_binary(self) -> None:
        class Bare:
            def read(self, size: int = -1, /) -> bytes:
                return BYTES[:size]

        assert ext.binary_read_exactly(Bare(), 4) == BYTES[:4]

    def test_bare_reader_works_as_text(self) -> None:
        class Bare:
            def read(self, size: int = -1, /) -> str:
                return TEXT[:size]

        assert ext.text_read_chars(Bare(), 4) == TEXT[:4]

    def test_a_text_writer_needs_nothing_but_write(self) -> None:
        class WriteOnly:
            def __init__(self) -> None:
                self.chunks: list[str] = []

            def write(self, data: str, /) -> int:
                self.chunks.append(data)
                return len(data)

        writer = WriteOnly()
        assert ext.text_write(writer, TEXT) == len(TEXT)
        assert "".join(writer.chunks) == TEXT

    def test_getting_it_wrong_says_what_to_do(self) -> None:
        class SilentlyText:
            def read(self, size: int | None = -1, /) -> str:
                return TEXT[:size] if size is not None and size >= 0 else TEXT

        with pytest.raises(OSError, match=r"did not return bytes.*use a text wrapper"):
            # intentional payload mismatch: caught at the first read, not at extraction
            ext.binary_read_exactly(SilentlyText(), 4)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_the_reverse_too(self) -> None:
        class SilentlyBinary:
            def read(self, size: int | None = -1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        with pytest.raises(OSError, match=r"did not return str.*use a binary wrapper"):
            # intentional payload mismatch: caught at the first read, not at extraction
            ext.text_read_chars(SilentlyBinary(), 4)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]


class TestMisleadingInheritance:
    """Deriving from the wrong half of `io` is refused; wrapping instead of inheriting works."""

    def test_a_structurally_misleading_object_is_refused(self) -> None:
        class TextWriterInBinaryClothing(io.RawIOBase):
            def __init__(self) -> None:
                self.written = ""

            def writable(self) -> bool:
                return True

            # intentional: contradicts the base, which types `write` as bytes-only
            def write(self, data: str, /) -> int:  # type: ignore[override]  # pyright: ignore[reportIncompatibleMethodOverride]
                self.written += data
                return len(data)

        with pytest.raises(TypeError, match=r"io\.RawIOBase"):
            # No ignore needed: statically this satisfies the text protocol; only the
            # runtime hierarchy check refuses it, which is exactly what is asserted here.
            ext.text_write(TextWriterInBinaryClothing(), TEXT)

    def test_wrapping_instead_of_inheriting_is_accepted(self) -> None:
        class ThirdPartyWriter(io.RawIOBase):
            def __init__(self) -> None:
                self.written = ""

            def writable(self) -> bool:
                return True

            # intentional: contradicts the base, which types `write` as bytes-only
            def write(self, data: str, /) -> int:  # type: ignore[override]  # pyright: ignore[reportIncompatibleMethodOverride]
                self.written += data
                return len(data)

        class TextShim:
            def __init__(self, inner: Any) -> None:
                self._inner = inner

            def write(self, data: str, /) -> Any:
                return self._inner.write(data)

            def flush(self) -> None:
                pass

        inner = ThirdPartyWriter()
        assert ext.text_write(TextShim(inner), TEXT) == len(TEXT)
        assert inner.written == TEXT


class TestDirectionChecks:
    """`io.IOBase` has every method, so its `readable()`/`writable()`/`seekable()` decide."""

    def test_a_write_only_file_is_refused_as_a_reader(self, tmp_path: Path) -> None:
        path = tmp_path / "w.bin"
        with open(path, "wb") as handle:
            with pytest.raises(TypeError, match=r"readable\(\) is False"):
                ext.binary_read_all(handle)

    def test_a_read_only_file_is_refused_as_a_writer(self, files: dict[str, Path]) -> None:
        with open(files["bin"], "rb") as handle:
            with pytest.raises(TypeError, match=r"writable\(\) is False"):
                ext.binary_write(handle, BYTES)

    def test_the_same_for_text(self, tmp_path: Path, files: dict[str, Path]) -> None:
        path = tmp_path / "w.txt"
        with open(path, "w", encoding="utf-8") as handle:
            with pytest.raises(TypeError, match=r"readable\(\) is False"):
                ext.text_read_all(handle)
        with open(files["txt"], encoding="utf-8") as handle:
            with pytest.raises(TypeError, match=r"writable\(\) is False"):
                ext.text_write(handle, TEXT)

    def test_an_unseekable_stream_is_refused_as_seekable(self) -> None:
        left, right = socket.socketpair()
        try:
            handle = left.makefile("rb")
            with pytest.raises(TypeError, match=r"seekable\(\) is False"):
                ext.binary_seek_roundtrip(handle)
        finally:
            left.close()
            right.close()

    def test_a_read_write_file_satisfies_both(self, tmp_path: Path) -> None:
        path = tmp_path / "rw.bin"
        path.write_bytes(b"")
        with open(path, "w+b") as handle:
            assert ext.binary_read_write(handle, BYTES) == 0

    def test_the_message_names_the_capability(self, files: dict[str, Path]) -> None:
        with open(files["bin"], "rb") as handle:
            with pytest.raises(TypeError, match="WRITE capability"):
                ext.binary_write(handle, BYTES)

    def test_duck_typed_objects_are_not_asked(self) -> None:

        class LiesAboutItself:
            def readable(self) -> bool:
                raise AssertionError("should never be called")

            def read(self, size: int | None = -1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        assert ext.binary_read_exactly(LiesAboutItself(), 4) == BYTES[:4]

    def test_an_iobase_that_raises_from_the_query_is_not_rejected(self) -> None:
        class Awkward(io.RawIOBase):
            def readable(self) -> bool:
                raise ValueError("cannot say")

            def read(self, size: int | None = -1, /) -> bytes:
                return BYTES[:size] if size is not None and size >= 0 else BYTES

        assert ext.binary_read_exactly(Awkward(), 4) == BYTES[:4]
