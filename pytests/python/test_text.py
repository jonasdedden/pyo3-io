"""Text payloads: characters cross the boundary as characters, counted as characters."""

import io
from pathlib import Path

import pytest
from helpers import DuckTextReader, DuckTextWriter, PartialTextWriter

import pyo3_io_tests as ext

# 1, 2, 3 and 4 byte characters, so any byte/character confusion shows up immediately.
MIXED = "aä€\U0001f600b"
ASCII = "hello world"


@pytest.fixture
def tmp_text(tmp_path: Path) -> Path:
    path = tmp_path / "data.txt"
    path.write_text(MIXED, encoding="utf-8")
    return path


class TestReadAll:
    def test_stringio(self) -> None:
        """pyo3-file cannot read a StringIO at all: `read_to_end` trips its 4-byte guard."""
        assert ext.text_read_all(io.StringIO(MIXED)) == MIXED

    def test_real_file(self, tmp_text: Path) -> None:
        with open(tmp_text, encoding="utf-8") as handle:
            assert ext.text_read_all(handle) == MIXED

    def test_duck_typed(self) -> None:
        assert ext.text_read_all(DuckTextReader(MIXED)) == MIXED

    def test_empty(self) -> None:
        assert ext.text_read_all(io.StringIO("")) == ""

    def test_short_string(self) -> None:
        """One character, well under the 4-byte buffer pyo3-file insists on."""
        assert ext.text_read_all(io.StringIO("a")) == "a"

    def test_large(self) -> None:
        text = MIXED * 20_000
        assert ext.text_read_all(io.StringIO(text)) == text

    def test_non_utf8_encoding_is_the_callers_choice(self, tmp_path: Path) -> None:
        """The encoding is decided by Python, where `encoding=` lives, not guessed here."""
        path = tmp_path / "latin1.txt"
        path.write_text("éèü", encoding="latin-1")
        with open(path, encoding="latin-1") as handle:
            assert ext.text_read_all(handle) == "éèü"

    def test_read_to_string_supports_positive_size_only(self) -> None:
        class PositiveReader(DuckTextReader):
            def read(self, size: int = -1, /) -> str:
                assert size > 0
                return super().read(size)

        text = ASCII * 1000
        reader = PositiveReader(text)
        assert ext.text_read_all(reader) == text

    def test_newlines_are_not_mangled(self) -> None:
        text = "a\nb\nc\n"
        assert ext.text_read_all(io.StringIO(text)) == text


class TestReadChars:
    @pytest.mark.parametrize("n", [0, 1, 2, 3, 4, 5])
    def test_counts_characters_not_bytes(self, n: int) -> None:
        """`read(3)` on `aä€😀b` is three characters, which is six UTF-8 bytes."""
        got = ext.text_read_chars(io.StringIO(MIXED), n)
        assert got == MIXED[:n]
        assert len(got) == n

    def test_single_character_needs_no_minimum_buffer(self) -> None:
        assert ext.text_read_chars(io.StringIO(MIXED), 1) == "a"

    def test_four_byte_character(self) -> None:
        assert ext.text_read_chars(io.StringIO("\U0001f600"), 1) == "\U0001f600"

    def test_past_end(self) -> None:
        assert ext.text_read_chars(io.StringIO("ab"), 100) == "ab"


class TestWrite:
    def test_stringio(self) -> None:
        buffer = io.StringIO()
        assert ext.text_write(buffer, MIXED) == len(MIXED)
        assert buffer.getvalue() == MIXED

    def test_duck_typed(self) -> None:
        writer = DuckTextWriter()
        ext.text_write(writer, MIXED)
        assert writer.value == MIXED
        assert writer.flushes == 1

    def test_partial_writes_are_retried_by_character(self) -> None:
        """The retry has to advance by characters, not bytes, or it splits a code point."""
        writer = PartialTextWriter()
        ext.text_write(writer, MIXED * 3)
        assert writer.value == MIXED * 3

    def test_real_file(self, tmp_path: Path) -> None:
        path = tmp_path / "out.txt"
        with open(path, "w", encoding="utf-8") as handle:
            ext.text_write(handle, MIXED)
        assert path.read_text(encoding="utf-8") == MIXED

    def test_written_in_the_callers_encoding(self, tmp_path: Path) -> None:
        path = tmp_path / "out-latin1.txt"
        with open(path, "w", encoding="latin-1") as handle:
            ext.text_write(handle, "éèü")
        assert path.read_bytes() == bytes([0xE9, 0xE8, 0xFC])

    def test_empty(self) -> None:
        buffer = io.StringIO()
        assert ext.text_write(buffer, "") == 0


class TestSeek:
    def test_cookie_roundtrip(self, tmp_text: Path) -> None:
        with open(tmp_text, encoding="utf-8") as handle:
            first, again = ext.text_seek_roundtrip(handle, 3)
        assert first == again == MIXED[:3]

    def test_stringio(self) -> None:
        first, again = ext.text_seek_roundtrip(io.StringIO(MIXED), 2)
        assert first == again == MIXED[:2]


class TestFileno:
    def test_real_file(self, tmp_text: Path) -> None:
        with open(tmp_text, encoding="utf-8") as handle:
            assert ext.text_fileno(handle) == handle.fileno()

    def test_stringio_is_an_error_not_a_panic(self) -> None:
        with pytest.raises(OSError) as excinfo:
            ext.text_fileno(io.StringIO("abc"))
        assert "PanicException" not in type(excinfo.value).__name__


class TestCombined:
    def test_read_write(self) -> None:
        buffer = io.StringIO()
        assert ext.text_read_write(buffer, MIXED) == 0  # cursor is at the end
        assert buffer.getvalue() == MIXED
