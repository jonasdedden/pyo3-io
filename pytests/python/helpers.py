"""Duck-typed objects: nothing here inherits from `io`, so only structure decides."""


class DuckBinaryReader:
    """`read(n) -> bytes`, and nothing else."""

    def __init__(self, data: bytes) -> None:
        self.data = data
        self.calls = 0

    def read(self, size: int = -1, /) -> bytes:
        self.calls += 1
        if size < 0:
            chunk, self.data = self.data, b""
        else:
            chunk, self.data = self.data[:size], self.data[size:]
        return chunk


class DuckTextReader:
    """`read(n) -> str`, and nothing else."""

    def __init__(self, text: str) -> None:
        self.text = text
        self.calls = 0

    def read(self, size: int = -1, /) -> str:
        self.calls += 1
        if size < 0:
            chunk, self.text = self.text, ""
        else:
            chunk, self.text = self.text[:size], self.text[size:]
        return chunk


class DuckBinaryWriter:
    def __init__(self) -> None:
        self.chunks: list[bytes] = []
        self.flushes = 0

    def write(self, data: bytes, /) -> int:
        if not isinstance(data, bytes):
            raise TypeError(f"expected bytes, got {type(data).__name__}")
        self.chunks.append(data)
        return len(data)

    def flush(self) -> None:
        self.flushes += 1

    @property
    def value(self) -> bytes:
        return b"".join(self.chunks)


class DuckTextWriter:
    def __init__(self) -> None:
        self.chunks: list[str] = []
        self.flushes = 0

    def write(self, data: str, /) -> int:
        if not isinstance(data, str):
            raise TypeError(f"expected str, got {type(data).__name__}")
        self.chunks.append(data)
        return len(data)

    def flush(self) -> None:
        self.flushes += 1

    @property
    def value(self) -> str:
        return "".join(self.chunks)


class Overreader:
    """Ignores the requested size and returns more than it was asked for."""

    def read(self, size: int, /) -> bytes:
        return b"x" * (size + 10) if size else b""


class NonBlocking:
    """Returns None, which Python streams use for "nothing available yet".

    `read(0)` still answers immediately, as every real stream does: a zero-length read never has
    to wait for anything.
    """

    def read(self, size: int, /) -> bytes | None:
        return b"" if size == 0 else None


class PartialWriter:
    """Accepts at most 3 bytes per call, like a real short write."""

    def __init__(self) -> None:
        self.chunks: list[bytes] = []

    def write(self, data: bytes, /) -> int:
        chunk = data[:3]
        self.chunks.append(chunk)
        return len(chunk)

    def flush(self) -> None:
        pass

    @property
    def value(self) -> bytes:
        return b"".join(self.chunks)


class PartialTextWriter:
    """Accepts at most 3 characters per call."""

    def __init__(self) -> None:
        self.chunks: list[str] = []

    def write(self, data: str, /) -> int:
        chunk = data[:3]
        self.chunks.append(chunk)
        return len(chunk)

    def flush(self) -> None:
        pass

    @property
    def value(self) -> str:
        return "".join(self.chunks)


class LiesAboutWrite:
    """Claims to have written more than it was given."""

    def write(self, data: bytes, /) -> int:
        return len(data) + 5

    def flush(self) -> None:
        pass
