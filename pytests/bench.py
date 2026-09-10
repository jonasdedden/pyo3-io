#!/usr/bin/env python3
"""Times `pyo3-file-typed` against `pyo3-file` on the same work.

The typed interface should be faster for two reasons, both of which come out of making the
payload kind part of the type:

  * binary `read_to_end` is one `read(-1)` instead of a call per buffer-sized chunk, because the
    implementation knows it is talking to a byte stream and can use Python's own "read it all";
  * text reads hand a `str` straight across, instead of encoding it to UTF-8 on the Python side
    and validating it back into a `String` on the Rust side.
"""

import gc
import io
import os
import statistics
import sys
import sysconfig
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT / "python"))
import conftest  # noqa: E402,F401  (stages the freshly built extension)

import pyo3_file_typed_tests as ext  # noqa: E402


def measure(fn, *, repeat: int, inner: int) -> float:
    """Best-of-`repeat` mean nanoseconds per call, to shrug off scheduler noise.

    The result is kept alive across an iteration on purpose. Dropping a multi-megabyte buffer
    immediately makes glibc return it to the OS and fault it back in on the next call, which
    swamps the thing being measured and does so differently depending on how the buffer grew.
    Real callers do something with what they read.
    """
    best = float("inf")
    keep = None
    for _ in range(repeat):
        gc.collect()
        start = time.perf_counter_ns()
        for _ in range(inner):
            keep = fn()
        best = min(best, (time.perf_counter_ns() - start) / inner)
    del keep
    return best


def cell(value: float | None) -> str:
    return f"{value / 1000:9.1f} us" if value is not None else f"{'n/a':>12}"


def ratio(typed: float, other: float | None) -> str:
    if other is None:
        return ""
    factor = other / typed
    return f"{factor:.2f}x" if factor >= 1 else f"1/{1 / factor:.2f}x"


def report(name: str, typed: float, file: float | None, filelike: float | None, note="") -> None:
    print(
        f"  {name:<20}{cell(typed)}{cell(file)}{cell(filelike)}"
        f"   {ratio(typed, file):>7} {ratio(typed, filelike):>8}  {note}"
    )


def try_measure(fn, **kwargs) -> tuple[float | None, str]:
    """Times `fn`, or reports why it cannot run at all."""
    try:
        fn()
    except BaseException as err:  # noqa: BLE001 - a panic is a BaseException
        return None, str(err).splitlines()[0][:44]
    return measure(fn, **kwargs), ""


def main() -> None:
    print(f"  {'':20}{'typed':>12}{'pyo3-file':>12}{'filelike':>12}   {'vs file':>7} {'vs flike':>8}")
    opts = dict(repeat=3, inner=20)

    print("\nbinary: read a whole stream")
    for size in (1 << 12, 1 << 16, 1 << 20, 1 << 23):
        data = os.urandom(size)
        typed = measure(lambda: ext.binary_read_all(io.BytesIO(data)), **opts)
        file, _ = try_measure(lambda: ext.legacy_read_all(io.BytesIO(data)), **opts)
        filelike, _ = try_measure(lambda: ext.filelike_read_all(io.BytesIO(data)), **opts)
        report(f"{size >> 10} KiB", typed, file, filelike)

    print("\ntext: read n characters")
    for chars in (1 << 10, 1 << 14, 1 << 18):
        text = "hello world " * (chars // 12 + 1)
        typed = measure(lambda: ext.text_read_chars(io.StringIO(text), chars), **opts)
        file, _ = try_measure(lambda: ext.legacy_read_chars(io.StringIO(text), chars), **opts)
        report(f"{chars} chars", typed, file, None, "pyo3-filelike: bytes only")

    print("\ntext: read a whole stream")
    for size in (1 << 12, 1 << 16, 1 << 20):
        text = "hello world " * (size // 12)
        typed = measure(lambda: ext.text_read_all(io.StringIO(text)), **opts)
        file, note = try_measure(lambda: ext.legacy_read_all_text(io.StringIO(text)), **opts)
        filelike, _ = try_measure(lambda: ext.filelike_text_read_all(io.StringIO(text)), **opts)
        report(f"{size >> 10} KiB", typed, file, filelike, f"pyo3-file: {note}" if note else "")

    print("\nconstruction: the per-extraction checks")
    typed = measure(lambda: ext.binary_read_exactly(io.BytesIO(b""), 0), repeat=5, inner=2000)
    file, _ = try_measure(lambda: ext.legacy_read_once(io.BytesIO(b"")), repeat=5, inner=2000)
    filelike, _ = try_measure(lambda: ext.filelike_read_once(io.BytesIO(b"")), repeat=5, inner=2000)
    report("empty read", typed, file, filelike)


if __name__ == "__main__":
    main()
