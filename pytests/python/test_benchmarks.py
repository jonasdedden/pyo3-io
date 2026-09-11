"""Contract tests for the benchmark helpers; no pytest dependency."""
import contextlib
import importlib.util
import io
import sys
import unittest
from pathlib import Path
from types import SimpleNamespace

import pyo3_typed_io_tests as ext

IMPLEMENTATIONS: tuple[str, ...] = ("typed-detached", "typed-bound", "legacy", "filelike")


class ObservedBytesIO(io.BytesIO):
    def __init__(self, data: bytes = b"") -> None:
        super().__init__(data)
        self.reads = 0
        self.flushes = 0

    def read(self, size: int | None = -1) -> bytes:
        self.reads += 1
        return super().read(size)

    def flush(self) -> None:
        self.flushes += 1
        return super().flush()


class BenchmarkTests(unittest.TestCase):
    def test_construction_does_not_read(self) -> None:
        for implementation in (*IMPLEMENTATIONS, "legacy-checked"):
            stream = ObservedBytesIO(b"abc")
            ext.bench_construct(stream, implementation, 3)
            self.assertEqual(stream.reads, 0)
            self.assertEqual(stream.tell(), 0)

    def test_binary_content_and_count(self) -> None:
        data = bytes(range(256)) * 17
        for implementation in IMPLEMENTATIONS:
            for chunk in (0, 1, 37, 4096):
                with self.subTest(implementation=implementation, chunk=chunk):
                    self.assertEqual(
                        ext.bench_read(io.BytesIO(data), implementation, chunk, True),
                        (len(data), data),
                    )
                    self.assertEqual(
                        ext.bench_read(io.BytesIO(data), implementation, chunk, False),
                        (len(data), None),
                    )

    def test_write_repetition_and_flush(self) -> None:
        for implementation in IMPLEMENTATIONS:
            for flush in (False, True):
                stream = ObservedBytesIO()
                self.assertEqual(ext.bench_write(stream, implementation, b"\x00\xff", 7, flush), 14)
                self.assertEqual(stream.getvalue(), b"\x00\xff" * 7)
                self.assertEqual(stream.flushes, int(flush))

    def test_typed_unicode_whole_stream(self) -> None:
        text = "héλ🙂" * 4000
        for implementation in IMPLEMENTATIONS[:2]:
            self.assertEqual(ext.bench_text_read(io.StringIO(text), implementation), text)

    def test_invalid_parameters(self) -> None:
        for call in (
            lambda: ext.bench_construct(io.BytesIO(), "typed-bound", 0),
            lambda: ext.bench_construct(io.BytesIO(), "unknown", 1),
            lambda: ext.bench_read(io.BytesIO(), "unknown", 1, False),
            lambda: ext.bench_read(io.BytesIO(), "typed-bound", sys.maxsize + 1, False),
            lambda: ext.bench_write(io.BytesIO(), "unknown", b"a", 1, False),
            lambda: ext.bench_write(io.BytesIO(), "typed-bound", b"a", 0, False),
            lambda: ext.bench_write(io.BytesIO(), "typed-bound", b"", 1, False),
            lambda: ext.bench_text_read(io.StringIO(), "unknown"),
        ):
            with self.assertRaises(ValueError):
                call()

    def test_incorrect_case_is_not_timed(self) -> None:
        path = Path(__file__).resolve().parents[1] / "bench.py"
        spec = importlib.util.spec_from_file_location("benchmark_cli", path)
        assert spec is not None
        assert spec.loader is not None
        bench = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(bench)
        calls = []
        output = io.StringIO()
        with contextlib.redirect_stdout(output):
            bench.report("bad", {"bad-adapter": (
                lambda: None,
                lambda: calls.append("timed"),
                lambda: bench.check_equal(b"wrong", b"expected"),
            )}, SimpleNamespace(warmup=1, repeats=2, iterations=2))
        self.assertEqual(calls, [])
        self.assertIn("UNSUPPORTED/BROKEN", output.getvalue())
        self.assertNotIn(" us ", output.getvalue())


if __name__ == "__main__":
    unittest.main()
