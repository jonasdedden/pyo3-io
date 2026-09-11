"""Construction checks callability and known I/O contracts without probing operations."""

import io

import pytest

import pyo3_file_typed_tests as ext


@pytest.mark.parametrize("value", [None, 42, b"read"])
def test_noncallable_read_and_write_are_rejected(value):
    class Stream:
        read = value
        write = value

    with pytest.raises(TypeError, match="missing or not callable"):
        ext.binary_read_exactly(Stream(), 0)
    with pytest.raises(TypeError, match="missing or not callable"):
        ext.binary_write(Stream(), b"")


def test_seek_tell_and_fileno_must_also_be_callable():
    class Stream:
        read = lambda self, size: b""
        seek = None
        fileno = None

    with pytest.raises(TypeError, match=r"\.seek\(\)"):
        ext.binary_seek_roundtrip(Stream())
    with pytest.raises(TypeError, match=r"\.fileno\(\)"):
        ext.binary_fileno(Stream())

    class TextStream:
        read = lambda self, size: ""
        seek = lambda self, offset, whence: 0
        tell = None

    with pytest.raises(TypeError, match=r"\.tell\(\)"):
        ext.text_seek_roundtrip(TextStream(), 0)


def test_dynamic_callables_are_accepted_without_probes_or_unrequested_lookups():
    class Stream:
        def __init__(self):
            self.lookups = []

        def __getattr__(self, name):
            self.lookups.append(name)
            if name == "read":
                return lambda size: pytest.fail("constructor called read")
            raise AssertionError(f"unrequested lookup: {name}")

    stream = Stream()
    assert ext.binary_read_exactly(stream, 0) == b""
    assert stream.lookups == ["read"]

    class InstanceReader:
        pass

    instance = InstanceReader()
    instance.read = lambda size: b"abc"[:size]
    assert ext.binary_read_exactly(instance, 2) == b"ab"


def test_attribute_lookup_failure_is_not_reported_as_missing_method():
    class Stream:
        @property
        def read(self):
            raise RuntimeError("reader lookup failed")

    with pytest.raises(RuntimeError, match="reader lookup failed"):
        ext.binary_read_exactly(Stream(), 0)


def test_closed_iobase_is_rejected_before_any_read():
    stream = io.BytesIO()
    stream.close()
    with pytest.raises(ValueError, match="closed file"):
        ext.binary_read_exactly(stream, 0)


@pytest.mark.parametrize("answer", [None, 0, 1, "False"])
def test_nonboolean_iobase_queries_are_unknown(answer):
    class Stream(io.RawIOBase):
        closed = property(lambda self: answer)

        def readable(self):
            return answer

        def read(self, size):
            return b"x"[:size]

        def writable(self):
            pytest.fail("unrequested query")

        def seekable(self):
            pytest.fail("unrequested query")

    assert ext.binary_read_exactly(Stream(), 1) == b"x"


@pytest.mark.skipif(not hasattr(io, "Reader"), reason="io.Reader added in Python 3.14")
def test_protocol_registration_does_not_replace_an_actual_method():
    class Empty:
        pass

    io.Reader.register(Empty)
    io.Writer.register(Empty)
    with pytest.raises(TypeError, match=r"\.read\(\)"):
        ext.binary_read_exactly(Empty(), 0)
    with pytest.raises(TypeError, match=r"\.write\(\)"):
        ext.binary_write(Empty(), b"")
