#!/usr/bin/env python3
"""Correctness-gated adapter benchmarks (stdlib only).

Run `python pytests/bench.py --quick` for a release smoke run.
No implementation is assumed to be faster.
"""
import argparse
import gc
import io
import json
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from collections.abc import Callable
from pathlib import Path
from typing import Any, BinaryIO

IMPLEMENTATIONS: tuple[str, ...] = ("typed-detached", "typed-bound", "legacy", "filelike")
TYPED: tuple[str, ...] = IMPLEMENTATIONS[:2]
# pyo3-filelike has no text writer.
TEXT_WRITERS: tuple[str, ...] = IMPLEMENTATIONS[:3]

# A benchmark step: resetting, running, or checking. Results are discarded, so every
# step shares one shape and `report` stays oblivious to what each step returns.
# The ellipsis keeps the `name=name` closures used to freeze loop variables; every
# step is still invoked with no arguments.
Step = Callable[..., object]


def positive(value: str) -> int:
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def check_equal(actual: object, expected: object) -> None:
    if actual != expected:
        raise ValueError("incorrect result (excluded from timing)")


def check_construct(ext: Any, stream: BinaryIO, name: str, count: int) -> None:
    """Run one construction batch; `bench_construct` returns None by design."""
    ext.bench_construct(stream, name, count)


def report(label: str, cases: dict[str, tuple[Step, Step, Step]], args: argparse.Namespace, divisor: float = 1) -> None:
    """Cases are (reset, call, check); neither reset nor check is timed.

    Rotate implementation order between repeats and report median repeat means.
    Results are dropped inside the timed call, identically for every adapter.
    """
    if getattr(args, "filter", None) and args.filter not in label:
        return
    samples: dict[str, list[float]] = {}
    active: dict[str, tuple[Step, Step]] = {}
    for name, (reset, call, check) in cases.items():
        try:
            reset()
            check()
            for _ in range(args.warmup):
                reset()
                call()
        except BaseException as error:
            # PyO3's PanicException is a BaseException; preserve interrupts.
            if isinstance(error, (KeyboardInterrupt, SystemExit, GeneratorExit)):
                raise
            detail = str(error).splitlines()
            print(f"{label:<48} {name:<16} UNSUPPORTED/BROKEN: "
                  f"{type(error).__name__}: {detail[0] if detail else ''}")
            continue
        active[name] = (reset, call)
        samples[name] = []
    names = list(active)
    for repeat in range(args.repeats):
        offset = repeat % len(names) if names else 0
        for name in names[offset:] + names[:offset]:
            reset, call = active[name]
            gc.collect()
            elapsed = 0
            for _ in range(args.iterations):
                reset()
                start = time.perf_counter_ns()
                call()
                elapsed += time.perf_counter_ns() - start
            samples[name].append(elapsed / args.iterations / divisor / 1000)
    for name, values in samples.items():
        print(f"{label:<48} {name:<16} {statistics.median(values):10.3f} us "
              f"[{min(values):.3f}, {max(values):.3f}]")


def binary_cases(ext: Any, stream: BinaryIO, data: bytes, chunk: int) -> dict[str, tuple[Step, Step, Step]]:
    return {
        name: (
            lambda: stream.seek(0),
            lambda name=name: ext.bench_read(stream, name, chunk, False),
            lambda name=name: check_equal(
                ext.bench_read(stream, name, chunk, True), (len(data), data)),
        ) for name in IMPLEMENTATIONS
    }


def write_cases(ext: Any, stream: BinaryIO, block: bytes, count: int, flush: bool) -> dict[str, tuple[Step, Step, Step]]:
    def reset() -> None:
        stream.seek(0)
        stream.truncate()

    def check(name: str) -> None:
        check_equal(ext.bench_write(stream, name, block, count, flush), len(block) * count)
        stream.seek(0)
        check_equal(stream.read(), block * count)

    return {
        name: (
            reset,
            lambda name=name: ext.bench_write(stream, name, block, count, flush),
            lambda name=name: check(name),
        ) for name in IMPLEMENTATIONS
    }


def text_write_cases(ext: Any, block: str, count: int) -> dict[str, tuple[Step, Step, Step]]:
    # A fresh StringIO per reset: getvalue() in the untimed check would otherwise switch a reused
    # one out of its append mode, and every timed write after it would pay for StringIO's slower
    # path instead of the adapter's.
    stream = [io.StringIO()]

    def reset() -> None:
        stream[0] = io.StringIO()

    def check(name: str) -> None:
        check_equal(ext.bench_text_write(stream[0], name, block, count, False), len(block.encode()) * count)
        check_equal(stream[0].getvalue(), block * count)

    return {
        name: (
            reset,
            lambda name=name: ext.bench_text_write(stream[0], name, block, count, False),
            lambda name=name: check(name),
        ) for name in TEXT_WRITERS
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("release", "debug"), default="release")
    parser.add_argument("--quick", action="store_true", help="small CI/smoke workload")
    parser.add_argument("--memory", action="store_true", help="fresh-process peak RSS for binary bulk reads")
    parser.add_argument("--repeats", type=positive, help="repeat means (default 5)")
    parser.add_argument("--iterations", type=positive, help="calls per repeat (default 10)")
    parser.add_argument("--warmup", type=positive, default=2)
    parser.add_argument("--size", type=positive, help="binary bytes / text characters (default 1 Mi)")
    parser.add_argument("--chunks", type=positive, nargs="+", help="binary chunk sizes")
    parser.add_argument("--constructors", type=positive, help="constructions per batch")
    parser.add_argument("--filter", help="only run workloads whose label contains this text")
    args = parser.parse_args()
    args.repeats = args.repeats or (2 if args.quick else 5)
    args.iterations = args.iterations or (2 if args.quick else 10)
    size: int = args.size or (16384 if args.quick else 1048576)
    chunks: list[int] = args.chunks or ([64, 4096] if args.quick else [64, 4096, 65536])
    constructors: int = args.constructors or (100 if args.quick else 2000)
    sys.path.insert(0, str(Path(__file__).resolve().parent / "python"))
    from _extension import install
    extension_path = install(args.profile)
    import pyo3_io_tests as ext

    print(f"Python {sys.version.split()[0]} / {platform.platform()}")
    print(f"extension={extension_path}; profile={args.profile}")
    print(f"repeats={args.repeats}, iterations={args.iterations}, warmup={args.warmup}, "
          f"size={size}, chunks={chunks}, constructor batch={constructors}")
    if args.profile != "release":
        print("DEBUG BUILD: diagnostic only, not representative performance.")
    print("Times: median repeat mean [min, max], microseconds per complete operation.")
    print("Construction rows alone are per adapter. Inputs and seek/truncate resets are untimed.")
    print("Each I/O call constructs ONE adapter; chunks amortize that cost across a stream.")
    print("Typed 'detached' means owned adapter, NOT GIL release: calls enter from Python.")
    print("Legacy uses py_new; legacy-checked uses py_with_requirements(read=true).")
    print("Constructor guarantees differ: typed checks payload and capability, legacy-checked")
    print("checks method presence; filelike uses its own constructor. These are costs, not rankings.")
    print("Real files are buffered, warm/page-cached; flush is NOT fsync/durable storage.")
    print("Full binary reads allocate Rust output; chunk reads reuse a fixed buffer.")
    print("Timed binary reads return only a byte count; untimed checks compare every byte.")
    print("Text compares identical Unicode strings, never characters against bytes;")
    print("timed text reads return only a UTF-8 length, like binary reads.")
    print("Use --memory for fresh-process peak RSS; it is not per-adapter allocation cost.\n")

    data: bytes = (bytes(range(256)) * ((size + 255) // 256))[:size]
    with io.BytesIO(data) as memory, tempfile.TemporaryFile("w+b") as disk:
        disk.write(data)
        disk.flush()
        for source, stream in (("BytesIO", memory), ("temporary-file", disk)):
            report(f"{source} construction (read capability)", {
                name: (
                    lambda: None,
                    lambda name=name: ext.bench_construct(stream, name, constructors),
                    lambda name=name: check_construct(ext, stream, name, 1),
                ) for name in (*IMPLEMENTATIONS, "legacy-checked")
            }, args, divisor=constructors)
            for chunk in [0, *chunks]:
                report(f"{source} read {size}B chunk={chunk or 'all'}",
                       binary_cases(ext, stream, data, chunk), args)
            for chunk in [size, *chunks]:
                block = data[:min(chunk, size)]
                count = max(1, size // len(block))
                for flush in (False, True):
                    report(f"{source} write {len(block)}B x {count} flush={int(flush)}",
                           write_cases(ext, stream, block, count, flush), args)
    for kind, pattern in (("ASCII", "hello world\n"), ("Unicode", "héλ🙂\n")):
        text = (pattern * ((size + len(pattern) - 1) // len(pattern)))[:size]
        encoded = len(text.encode())
        with io.StringIO(text) as text_stream:
            for chunk in [0, *chunks]:
                report(f"{kind} text read {len(text)}ch/{encoded}B chunk={chunk or 'all'}", {
                    name: (
                        lambda: text_stream.seek(0),
                        lambda name=name, chunk=chunk: ext.bench_text_read(
                            text_stream, name, chunk, False),
                        lambda name=name, chunk=chunk: check_equal(
                            ext.bench_text_read(text_stream, name, chunk, True), (encoded, text)),
                    ) for name in (IMPLEMENTATIONS if chunk == 0 else TYPED)
                }, args)
        for chunk in [size, *chunks]:
            block = text[:min(chunk, size)]
            count = max(1, size // len(block))
            report(f"{kind} text write {len(block)}ch x {count}",
                   text_write_cases(ext, block, count), args)
    if args.memory:
        print("\nBinary bulk-read peak process RSS (one fresh process per case).")
        print("Includes Python, extension loading and input setup; before/after are high-water")
        print("marks, not current memory or allocated-byte counts. Small differences are noise.")
        for source in ("BytesIO", "temporary-file"):
            for name in IMPLEMENTATIONS:
                result = subprocess.run(
                    [
                        sys.executable, str(Path(__file__).with_name("memory.py")),
                        "--profile", args.profile, "--size", str(size),
                        "--source", source, "--implementation", name,
                    ],
                    capture_output=True, text=True, check=True,
                )
                report_data = json.loads(result.stdout)
                if "unavailable" in report_data:
                    print(f"{source} {name}: {report_data['unavailable']}")
                else:
                    before = report_data["before_peak_bytes"] / (1 << 20)
                    after = report_data["after_peak_bytes"] / (1 << 20)
                    print(f"{source:<20} {name:<16} before={before:.2f} MiB peak={after:.2f} MiB")


if __name__ == "__main__":
    main()
