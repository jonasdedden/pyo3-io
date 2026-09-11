"""Construction checks callability and known I/O contracts without probing operations."""

import io
from typing import Any

import pytest

import pyo3_typed_io_tests as ext


@pytest.mark.parametrize("value", [None, 42, b"read"])
def test_noncallable_read_and_write_are_rejected(value: Any) -> None:
    class Stream:
        read = value
        write = value

    with pytest.raises(TypeError, match="missing or not callable"):
        ext.binary_read_exactly(Stream(), 0)
    with pytest.raises(TypeError, match="missing or not callable"):
        ext.binary_write(Stream(), b"")


def test_seek_tell_and_fileno_must_also_be_callable() -> None:
    class Stream:
        read = lambda self, size: b""
        seek = None
        fileno = None

    with pytest.raises(TypeError, match=r"\.seek\(\)"):
        # intentional missing-method rejection
        ext.binary_seek_roundtrip(Stream())  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]
    with pytest.raises(TypeError, match=r"\.fileno\(\)"):
        # intentional missing-method rejection
        ext.binary_fileno(Stream())  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]

    class TextStream:
        read = lambda self, size: ""
        seek = lambda self, offset, whence: 0
        tell = None

    with pytest.raises(TypeError, match=r"\.tell\(\)"):
        # intentional missing-method rejection
        ext.text_seek_roundtrip(TextStream(), 0)  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]


def test_dynamic_callables_are_accepted_without_probes_or_unrequested_lookups() -> None:
    class Stream:
        def __init__(self) -> None:
            self.lookups: list[str] = []

        def __getattr__(self, name: str) -> Any:
            self.lookups.append(name)
            if name == "read":
                return lambda size: pytest.fail("constructor called read")
            raise AssertionError(f"unrequested lookup: {name}")

    stream = Stream()
    # intentional: `__getattr__` supplies `read` dynamically, which mypy accepts
    # via `Any` but basedpyright cannot see
    assert ext.binary_read_exactly(stream, 0) == b""  # pyright: ignore[reportArgumentType]
    assert stream.lookups == ["read"]

    class InstanceReader:
        pass

    instance = InstanceReader()
    setattr(instance, "read", lambda size: b"abc"[:size])
    # intentional: the instance attribute is invisible to both checkers
    assert ext.binary_read_exactly(instance, 2) == b"ab"  # type: ignore[arg-type]  # pyright: ignore[reportArgumentType]


def test_attribute_lookup_failure_is_not_reported_as_missing_method() -> None:
    class Stream:
        @property
        def read(self) -> Any:
            raise RuntimeError("reader lookup failed")

    with pytest.raises(RuntimeError, match="reader lookup failed"):
        # intentional: the lookup raises instead of returning a method
        ext.binary_read_exactly(Stream(), 0)  # pyright: ignore[reportArgumentType]


def test_closed_iobase_is_rejected_before_any_read() -> None:
    stream = io.BytesIO()
    stream.close()
    with pytest.raises(ValueError, match="closed file"):
        ext.binary_read_exactly(stream, 0)


@pytest.mark.parametrize("answer", [None, 0, 1, "False"])
def test_nonboolean_iobase_queries_are_unknown(answer: Any) -> None:
    class Stream(io.RawIOBase):
        # intentional non-bool `closed`: an unanswerable query must not reject.
        # mypy accepts the `Any`-valued property; basedpyright does not.
        closed = property(lambda self: answer)  # pyright: ignore[reportIncompatibleMethodOverride]

        def readable(self) -> Any:
            return answer

        def read(self, size: int = -1, /) -> bytes:
            return b"x"[:size]

        def writable(self) -> bool:
            pytest.fail("unrequested query")

        def seekable(self) -> bool:
            pytest.fail("unrequested query")

    assert ext.binary_read_exactly(Stream(), 1) == b"x"
