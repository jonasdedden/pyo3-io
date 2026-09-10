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


def report(name: str, typed_ns: float, legacy_ns: float | None, note: str = "") -> None:
    if legacy_ns is None:
        print(f"  {name:<34} {typed_ns / 1000:9.1f} us      {'n/a':>9}   {note}")
        return
    ratio = legacy_ns / typed_ns
    verdict = f"{ratio:.2f}x faster" if ratio >= 1 else f"{1 / ratio:.2f}x SLOWER"
    print(f"  {name:<34} {typed_ns / 1000:9.1f} us  {legacy_ns / 1000:9.1f} us   {verdict}")


def main() -> None:
    print(f"{'':36}{'typed':>12}{'pyo3-file':>13}")

    print("\nbinary: read a whole stream")
    for size in (1 << 12, 1 << 16, 1 << 20, 1 << 23):
        data = os.urandom(size)
        typed = measure(lambda: ext.binary_read_all(io.BytesIO(data)), repeat=3, inner=20)
        legacy = measure(lambda: ext.legacy_read_all(io.BytesIO(data)), repeat=3, inner=20)
        report(f"{size >> 10} KiB", typed, legacy)

    print("\ntext: read n characters")
    for chars in (1 << 10, 1 << 14, 1 << 18):
        text = "hello world " * (chars // 12 + 1)
        typed = measure(lambda: ext.text_read_chars(io.StringIO(text), chars), repeat=3, inner=20)
        legacy = measure(
            lambda: ext.legacy_read_chars(io.StringIO(text), chars), repeat=3, inner=20
        )
        report(f"{chars} chars", typed, legacy)

    print("\ntext: read a whole stream")
    for size in (1 << 12, 1 << 16, 1 << 20):
        text = "hello world " * (size // 12)
        typed = measure(lambda: ext.text_read_all(io.StringIO(text)), repeat=3, inner=20)
        try:
            ext.legacy_read_all_text(io.StringIO(text))
            legacy = measure(
                lambda: ext.legacy_read_all_text(io.StringIO(text)), repeat=3, inner=20
            )
            note = ""
        except Exception as err:
            legacy, note = None, f"pyo3-file: {err}"
        report(f"{size >> 10} KiB", typed, legacy, note)

    print("\nconstruction: the per-extraction checks")
    buffer = io.BytesIO(b"abc")
    typed = measure(lambda: ext.binary_read_exactly(buffer, 0), repeat=5, inner=2000)
    legacy = measure(lambda: ext.legacy_read_once(io.BytesIO(b"")), repeat=5, inner=2000)
    report("empty read", typed, legacy, "")


if __name__ == "__main__":
    main()
