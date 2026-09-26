"""The payload kind and the capability set are both checked at extraction."""

import io
from collections.abc import Callable
from pathlib import Path
from typing import Any

import pytest
from helpers import DuckBinaryReader, DuckTextReader

import pyo3_io_tests as ext


class TestModeGuard:
    """A stream of the wrong kind is refused with an actionable message."""

    @pytest.mark.parametrize(
        "obj",
        [io.StringIO("abc"), io.TextIOWrapper(io.BytesIO(b"abc"))],
        ids=["StringIO", "TextIOWrapper"],
    )
    def test_text_object_rejected_by_binary(self, obj: Any) -> None:
        with pytest.raises(TypeError, match="expected a binary file-like object"):
            ext.binary_read_all(obj)

    def test_binary_rejection_suggests_the_buffer(self) -> None:
        with pytest.raises(TypeError, match=r"obj\.buffer"):
            # intentional wrong-kind rejection
            ext.binary_read_all(io.StringIO("abc"))  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_real_text_file_rejected_by_binary(self, tmp_path: Path) -> None:
        path = tmp_path / "t.txt"
        path.write_text("abc")
        with open(path, encoding="utf-8") as handle:
            with pytest.raises(TypeError, match="expected a binary file-like object"):
                # intentional wrong-kind rejection
                ext.binary_read_all(handle)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    @pytest.mark.parametrize(
        "factory",
        [lambda: io.BytesIO(b"abc"), lambda: io.BufferedReader(io.BytesIO(b"abc"))],
        ids=["BytesIO", "BufferedReader"],
    )
    def test_binary_object_rejected_by_text(self, factory: Callable[[], Any]) -> None:
        with pytest.raises(TypeError, match="expected a text file-like object"):
            ext.text_read_all(factory())

    def test_text_rejection_suggests_textiowrapper(self) -> None:
        with pytest.raises(TypeError, match="io.TextIOWrapper"):
            # intentional wrong-kind rejection
            ext.text_read_all(io.BytesIO(b"abc"))  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_real_binary_file_rejected_by_text(self, tmp_path: Path) -> None:
        path = tmp_path / "t.bin"
        path.write_bytes(b"abc")
        with open(path, "rb") as handle:
            with pytest.raises(TypeError, match="expected a text file-like object"):
                # intentional wrong-kind rejection
                ext.text_read_all(handle)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_the_suggested_escape_hatches_work(self, tmp_path: Path) -> None:
        """Both directions are one call away, on the Python side where encoding belongs."""
        path = tmp_path / "t.txt"
        path.write_text("héllo", encoding="utf-8")
        with open(path, encoding="utf-8") as text:
            assert ext.binary_read_all(text.buffer) == "héllo".encode()
        with open(path, "rb") as binary:
            assert ext.text_read_all(io.TextIOWrapper(binary, encoding="utf-8")) == "héllo"

    def test_duck_typed_objects_are_taken_at_their_word(self) -> None:
        """Only real `io` classes are rejected; structure decides for everything else."""
        assert ext.binary_read_all(DuckBinaryReader(b"abc")) == b"abc"
        assert ext.text_read_all(DuckTextReader("abc")) == "abc"


class TestCapabilityChecks:
    """Every method the Rust side will call is checked up front, by name."""

    def test_missing_read(self) -> None:
        with pytest.raises(TypeError, match=r"has no \.read\(\) method"):
            # intentional missing-method rejection
            ext.binary_read_all(object())  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_missing_write(self) -> None:
        class NoWrite:
            def flush(self) -> None: ...

        with pytest.raises(TypeError, match=r"has no \.write\(\) method"):
            # intentional missing-method rejection
            ext.binary_write(NoWrite(), b"x")  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_flush_is_not_required(self) -> None:
        """An object with no `flush` has nothing to flush, so it is accepted and not called."""

        class NoFlush:
            def __init__(self) -> None:
                self.chunks: list[bytes] = []

            def write(self, data: bytes, /) -> int:
                self.chunks.append(data)
                return len(data)

        writer = NoFlush()
        assert ext.binary_write(writer, b"xyz") == 3
        assert b"".join(writer.chunks) == b"xyz"

    def test_missing_seek(self) -> None:
        class NoSeek:
            def read(self, size: int, /) -> bytes:
                return b""

        with pytest.raises(TypeError, match=r"has no \.seek\(\) method"):
            # intentional missing-method rejection
            ext.binary_seek_roundtrip(NoSeek())  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_missing_tell_for_text_seek(self) -> None:
        """Text positions are opaque cookies, so seeking one needs `tell` as well."""

        class NoTell:
            def read(self, size: int, /) -> str:
                return ""

            def seek(self, offset: int, whence: int, /) -> int:
                return 0

        with pytest.raises(TypeError, match=r"has no \.tell\(\) method"):
            # intentional missing-method rejection
            ext.text_seek_roundtrip(NoTell(), 1)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_missing_fileno(self) -> None:
        class NoFileno:
            def read(self, size: int, /) -> bytes:
                return b""

        with pytest.raises(TypeError, match=r"has no \.fileno\(\) method"):
            # intentional missing-method rejection
            ext.binary_fileno(NoFileno())  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    def test_capabilities_not_asked_for_are_not_required(self) -> None:
        """A read-only function must not demand `write`, `seek` or `fileno`."""

        class ReadOnly:
            def read(self, size: int = -1, /) -> bytes:
                return b"" if size else b""

        assert ext.binary_read_all(ReadOnly()) == b""
