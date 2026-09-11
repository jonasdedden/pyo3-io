"""Side by side with `pyo3-filelike`, which already splits binary from text.

It is the closer of the two neighbours: `PyBinaryFile` and `PyTextFile` make the payload kind
static, `PyTextFile` correctly has no `Seek`, and its internal buffer means text reads work at
every length, which is exactly what `pyo3-file` gets wrong. What is left are the places where the
split is not carried all the way through.

Each test asserts both halves, so it fails if `pyo3-filelike` changes.
"""

import io

import pytest

import pyo3_typed_io_tests as ext

LATIN1_TEXT = "éèü" * 8
LATIN1_BYTES = LATIN1_TEXT.encode("latin-1")


@pytest.fixture
def latin1_file(tmp_path):
    path = tmp_path / "latin1.txt"
    path.write_bytes(LATIN1_BYTES)
    return path


class TestWhatItAlreadyGetsRight:
    """Credit where it is due; these are regressions if this crate ever does worse."""

    @pytest.mark.parametrize("n", [0, 1, 5, 37, 1000])
    def test_text_reads_work_at_every_length(self, n):
        """The buffering fixes `pyo3-file`'s 4-byte guard outright."""
        assert len(ext.filelike_text_read_all(io.StringIO("x" * n))) == n
        assert len(ext.text_read_all(io.StringIO("x" * n))) == n

    def test_binary_reads_are_exact(self, latin1_file):
        with open(latin1_file, "rb") as handle:
            assert ext.filelike_read_all(handle) == LATIN1_BYTES
        with open(latin1_file, "rb") as handle:
            assert ext.binary_read_all(handle) == LATIN1_BYTES


class TestTextStillReachesByteConsumers:
    """`PyTextFile` implements `std::io::Read`, so the split stops at the type name."""

    def test_pyo3_filelike_hands_over_re_encoded_bytes(self, latin1_file):
        with open(latin1_file, encoding="latin-1") as handle:
            got = ext.filelike_text_as_bytes(handle)
        assert got != LATIN1_BYTES
        assert got == LATIN1_TEXT.encode("utf-8")
        assert len(got) == 2 * len(LATIN1_BYTES)

    def test_typed_has_no_such_path(self, latin1_file):
        """There is no way to reach `io::Read` from a text file here; see
        tests/ui/io_read_on_text.rs and tests/comparison.rs."""
        with open(latin1_file, encoding="latin-1") as handle:
            assert ext.text_read_all(handle) == LATIN1_TEXT
        with open(latin1_file, "rb") as handle:
            assert ext.binary_read_all(handle) == LATIN1_BYTES


class TestKindDetectionIsAHeuristic:
    """`PyBinaryFile` checks the `mode` attribute and assumes binary when there is none."""

    def test_stringio_is_accepted_then_fails_at_read(self):
        """`io.StringIO` has no `mode`, so it gets through the check."""
        with pytest.raises(OSError, match="not an instance of 'bytes'"):
            ext.filelike_read_once(io.StringIO("abc"))

    def test_duck_typed_text_reader_is_accepted_then_fails_at_read(self):
        class DuckText:
            def read(self, size=-1, /) -> str:
                return "abc"

        with pytest.raises(OSError, match="not an instance of 'bytes'"):
            ext.filelike_read_once(DuckText())

    def test_typed_refuses_stringio_up_front_with_a_way_out(self):
        with pytest.raises(TypeError, match=r"expected a binary file-like object.*obj\.buffer"):
            ext.binary_read_all(io.StringIO("abc"))

    def test_typed_refuses_duck_typed_text_at_the_type_checker(self, stub_source):
        """Structure decides, so this one is caught before it runs at all: `SupportsBinaryRead`
        wants `read(...) -> ReadableBuffer | None`, which a `-> str` reader is not. See
        typecheck/check_bad.py, case 2."""
        assert "def read(self, size: int, /) -> ReadableBuffer | None: ..." in stub_source

    def test_pytextfile_checks_nothing_at_all(self):
        """Only `PyBinaryFile` has a mode check; a binary object is accepted as text."""
        assert ext.filelike_text_as_bytes(io.BytesIO(b"abc")) == b"abc"

    def test_typed_refuses_it(self):
        with pytest.raises(TypeError, match="expected a text file-like object"):
            ext.text_read_all(io.BytesIO(b"abc"))


class TestRejectionIsAPanic:
    """`PyBinaryFile::new` is private, so `From`'s `unwrap` is the only way in."""

    def test_pyo3_filelike_panics_on_a_text_mode_file(self, latin1_file):
        with open(latin1_file, encoding="latin-1") as handle:
            with pytest.raises(BaseException) as excinfo:
                ext.filelike_read_once(handle)
        assert "Panic" in type(excinfo.value).__name__

    def test_typed_returns_a_type_error(self, latin1_file):
        with open(latin1_file, encoding="latin-1") as handle:
            with pytest.raises(TypeError) as excinfo:
                ext.binary_read_all(handle)
        assert "Panic" not in type(excinfo.value).__name__


class TestFilenoPanics:
    """`AsFd` cannot fail either, so this is the same panic `pyo3-file` has."""

    def test_pyo3_filelike_panics(self):
        with pytest.raises(BaseException) as excinfo:
            ext.filelike_fileno(io.BytesIO(b"abc"))
        assert "Panic" in type(excinfo.value).__name__

    def test_typed_returns_an_error(self):
        with pytest.raises(OSError) as excinfo:
            ext.binary_fileno(io.BytesIO(b"abc"))
        assert "Panic" not in type(excinfo.value).__name__


class TestNoCapabilityTyping:
    """`PyBinaryFile` is `Read + Write + Seek` whatever the function needed."""

    def test_writing_a_read_only_handle_reaches_runtime(self, latin1_file):
        with open(latin1_file, "rb") as handle:
            with pytest.raises(Exception) as excinfo:
                ext.filelike_write(handle, b"x")
        assert "write" in str(excinfo.value)

    def test_typed_cannot_express_the_mistake(self):
        """`BinaryRead` is `Read` and nothing else; see tests/ui/write_on_read_only.rs."""
        assert not hasattr(ext, "binary_write_to_a_reader")


class TestAnnotationQuality:
    """Comparison helpers explicitly accept PyAny; the typed helpers declare their requirements."""

    def test_pyo3_filelike_entry_points_are_any(self, stub_source):
        for name in ("filelike_read_all", "filelike_read_once", "filelike_fileno"):
            assert f"def {name}(obj: Any)" in stub_source, name

    def test_typed_entry_points_name_what_they_need(self, stub_source):
        assert "def binary_read_all(file: SupportsBinaryRead) -> bytes" in stub_source
        assert "def text_read_all(file: SupportsTextRead) -> str" in stub_source
