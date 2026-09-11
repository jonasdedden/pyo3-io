"""Text extraction is UTF-8 checked; a by-value bulk result cannot preserve partial errors."""

import io

import pytest

import pyo3_typed_io_tests as ext


def test_surrogate_text_is_not_silently_replaced() -> None:
    with pytest.raises(OSError, match="UTF-8"):
        ext.text_read_chars(io.StringIO("\ud800"), 1)


def test_partial_text_before_would_block_is_consumed_not_rewound() -> None:
    class Reader:
        def __init__(self) -> None:
            self.parts = iter(["é", None, "remaining"])

        def read(self, size: int) -> str | None:
            return next(self.parts)

    stream = Reader()
    with pytest.raises(BlockingIOError):
        ext.text_read_all(stream)
    assert ext.text_read_chars(stream, 10) == "remaining"
