"""The payload kind and the capability set are both checked at extraction."""

import io

import pytest
from helpers import DuckBinaryReader, DuckTextReader

import pyo3_file_typed_tests as ext


class TestModeGuard:
    """A stream of the wrong kind is refused with an actionable message."""

    @pytest.mark.parametrize(
        "obj",
        [io.StringIO("abc"), io.TextIOWrapper(io.BytesIO(b"abc"))],
        ids=["StringIO", "TextIOWrapper"],
    )
    def test_text_object_rejected_by_binary(self, obj):
        with pytest.raises(TypeError, match="expected a binary file-like object"):
            ext.binary_read_all(obj)

    def test_binary_rejection_suggests_the_buffer(self):
        with pytest.raises(TypeError, match=r"obj\.buffer"):
            ext.binary_read_all(io.StringIO("abc"))

    def test_real_text_file_rejected_by_binary(self, tmp_path):
        path = tmp_path / "t.txt"
        path.write_text("abc")
        with open(path, encoding="utf-8") as handle:
            with pytest.raises(TypeError, match="expected a binary file-like object"):
                ext.binary_read_all(handle)

    @pytest.mark.parametrize(
        "factory",
        [lambda: io.BytesIO(b"abc"), lambda: io.BufferedReader(io.BytesIO(b"abc"))],
        ids=["BytesIO", "BufferedReader"],
    )
    def test_binary_object_rejected_by_text(self, factory):
        with pytest.raises(TypeError, match="expected a text file-like object"):
            ext.text_read_all(factory())

    def test_text_rejection_suggests_textiowrapper(self):
        with pytest.raises(TypeError, match="io.TextIOWrapper"):
            ext.text_read_all(io.BytesIO(b"abc"))

    def test_real_binary_file_rejected_by_text(self, tmp_path):
        path = tmp_path / "t.bin"
        path.write_bytes(b"abc")
        with open(path, "rb") as handle:
            with pytest.raises(TypeError, match="expected a text file-like object"):
                ext.text_read_all(handle)

    def test_the_suggested_escape_hatches_work(self, tmp_path):
        """Both directions are one call away, on the Python side where encoding belongs."""
        path = tmp_path / "t.txt"
        path.write_text("héllo", encoding="utf-8")
        with open(path, encoding="utf-8") as text:
            assert ext.binary_read_all(text.buffer) == "héllo".encode()
        with open(path, "rb") as binary:
            assert ext.text_read_all(io.TextIOWrapper(binary, encoding="utf-8")) == "héllo"

    def test_duck_typed_objects_are_taken_at_their_word(self):
        """Only real `io` classes are rejected; structure decides for everything else."""
        assert ext.binary_read_all(DuckBinaryReader(b"abc")) == b"abc"
        assert ext.text_read_all(DuckTextReader("abc")) == "abc"


class TestCapabilityChecks:
    """Every method the Rust side will call is checked up front, by name."""

    def test_missing_read(self):
        with pytest.raises(TypeError, match=r"has no \.read\(\) method"):
            ext.binary_read_all(object())

    def test_missing_write(self):
        class NoWrite:
            def flush(self): ...

        with pytest.raises(TypeError, match=r"has no \.write\(\) method"):
            ext.binary_write(NoWrite(), b"x")

    def test_missing_flush(self):
        class NoFlush:
            def write(self, data, /):
                return len(data)

        with pytest.raises(TypeError, match=r"has no \.flush\(\) method"):
            ext.binary_write(NoFlush(), b"x")

    def test_missing_seek(self):
        class NoSeek:
            def read(self, size, /):
                return b""

        with pytest.raises(TypeError, match=r"has no \.seek\(\) method"):
            ext.binary_seek_roundtrip(NoSeek())

    def test_missing_tell_for_text_seek(self):
        """Text positions are opaque cookies, so seeking one needs `tell` as well."""

        class NoTell:
            def read(self, size, /):
                return ""

            def seek(self, offset, whence, /):
                return 0

        with pytest.raises(TypeError, match=r"has no \.tell\(\) method"):
            ext.text_seek_roundtrip(NoTell(), 1)

    def test_missing_fileno(self):
        class NoFileno:
            def read(self, size, /):
                return b""

        with pytest.raises(TypeError, match=r"has no \.fileno\(\) method"):
            ext.binary_fileno(NoFileno())

    def test_capabilities_not_asked_for_are_not_required(self):
        """A read-only function must not demand `write`, `seek` or `fileno`."""

        class ReadOnly:
            def read(self, size=-1, /):
                return b"" if size else b""

        assert ext.binary_read_all(ReadOnly()) == b""
