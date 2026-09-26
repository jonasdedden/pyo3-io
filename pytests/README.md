# Validation and benchmarks

Run from the repository root with Rust, Python, and `uv` available:

```sh
cargo test --doc
python3 pytests/check_rustdoc.py
./pytests/run-tests.sh
```

The Python tools (pytest, ruff, mypy, basedpyright) are pinned in `pyproject.toml` and `uv.lock`; their settings live there too, so `uv run mypy` in `pytests/` is the same check CI runs. The runner defaults to standard Rust and Python validation, without a custom stub generator, and checks freshly rendered rustdoc pages and their links. Opt in to stub generation/type checking or benchmarks:

```sh
PYO3_INTROSPECTION=/path/to/pyo3-introspection ./pytests/run-tests.sh --stubs
# Alternatively supply PYO3_CHECKOUT=/path/to/custom/pyo3 for the generator.
./pytests/run-tests.sh --bench
python pytests/bench.py --quick
python pytests/bench.py --profile release
python pytests/bench.py --memory --size 8388608
```

`--stubs` requires the custom generator explicitly; there is no neighboring-checkout default. Stub checking also needs `pyright`. The Python benchmark defaults to a release build (built incrementally outside timing) and reports median and spread for constructor-only, chunk, bulk, real-file, and ASCII/Unicode text read and write workloads; `--filter TEXT` runs only the workloads whose label contains `TEXT`. Use its results for the measured workload, not as a fixed speed claim. `--memory` runs each binary bulk-read case in a fresh subprocess and reports peak process RSS on Linux/macOS. It includes Python, extension loading and input setup; it is not an allocated-byte count or a claim about the adapter alone.

## Measured results

Median of three `python pytests/bench.py --repeats 9` runs, each figure itself the median of nine repeat means over ten calls: release build, otherwise idle Intel Core Ultra 7 258V, Linux 7.2, Python 3.14.7, measured 2026-09-23. Times are microseconds per complete operation, and every I/O row includes constructing one adapter. Factors are relative to the bound typed adapter, so above 1× is slower than it.

"fails": `pyo3-file` raises `OSError: buffer size must be at least 4 bytes` reading a whole text stream, and panics on non-ASCII text writes because it counts the characters Python reports as bytes. "n/a": `pyo3-filelike` has no text writer, and neither crate reads a count of characters. These are this machine's numbers for these workloads, not a general ranking; rerun the benchmark for yours.

| Workload | typed, bound | typed, detached | `pyo3-file` | `pyo3-filelike` |
|---|---:|---:|---:|---:|
| **Binary, `io.BytesIO`, 1 MiB** | | | | |
| construction only (read) | 0.357 | 0.355 | 0.060 (0.17×) | 0.186 (0.52×) |
| `read_to_end` | 39.4 | 38.9 | 81.2 (2.06×) | 84.4 (2.14×) |
| `read`, 64 B buffer | 372.5 | 418.6 | 408.5 (1.10×) | 757.6 (2.03×) |
| `read`, 4,096 B buffer | 31.2 | 31.9 | 31.3 (1.00×) | 37.7 (1.21×) |
| `read`, 65,536 B buffer | 34.5 | 35.7 | 33.7 (0.98×) | 34.6 (1.00×) |
| `write_all` 1,048,576 B × 1 | 54.6 | 54.9 | 53.3 (0.98×) | 54.3 (0.99×) |
| `write_all` 64 B × 16,384 | 446.9 | 490.6 | 466.2 (1.04×) | 804.3 (1.80×) |
| `write_all` 4,096 B × 256 | 35.0 | 35.5 | 33.9 (0.97×) | 40.5 (1.16×) |
| `write_all` 65,536 B × 16 | 32.9 | 34.5 | 32.1 (0.97×) | 33.0 (1.00×) |
| `write_all` 64 B × 16,384, then `flush` | 448.5 | 494.3 | 467.5 (1.04×) | 808.9 (1.80×) |
| **Binary, buffered temporary file, 1 MiB** | | | | |
| construction only (read) | 0.383 | 0.382 | 0.060 (0.16×) | 0.052 (0.14×) |
| `read_to_end` | 96.0 | 96.5 | 127.9 (1.33×) | 128.3 (1.34×) |
| `read`, 64 B buffer | 439.5 | 491.1 | 482.5 (1.10×) | 832.3 (1.89×) |
| `read`, 4,096 B buffer | 67.5 | 68.7 | 67.4 (1.00×) | 74.0 (1.10×) |
| `read`, 65,536 B buffer | 70.3 | 71.3 | 69.4 (0.99×) | 71.6 (1.02×) |
| `write_all` 1,048,576 B × 1 | 125.3 | 126.2 | 124.7 (1.00×) | 123.4 (0.98×) |
| `write_all` 64 B × 16,384 | 800.3 | 865.3 | 841.4 (1.05×) | 1,191.3 (1.49×) |
| `write_all` 4,096 B × 256 | 115.5 | 116.1 | 114.5 (0.99×) | 120.9 (1.05×) |
| `write_all` 65,536 B × 16 | 112.6 | 113.0 | 112.2 (1.00×) | 113.7 (1.01×) |
| `write_all` 64 B × 16,384, then `flush` | 809.0 | 877.5 | 851.9 (1.05×) | 1,204.5 (1.49×) |
| **Text, ASCII, 1 Mi characters, `io.StringIO`** | | | | |
| read whole stream | 161.8 | 166.2 | fails | 362.7 (2.24×) |
| `read_chars(64)` until EOF | 644.1 | 696.0 | n/a | n/a |
| `read_chars(4,096)` until EOF | 174.9 | 176.6 | n/a | n/a |
| `read_chars(65,536)` until EOF | 210.7 | 211.1 | n/a | n/a |
| write 1,048,576 chars × 1 | 77.4 | 77.0 | 99.2 (1.28×) | n/a |
| write 64 chars × 16,384 | 554.6 | 626.2 | 629.5 (1.14×) | n/a |
| write 4,096 chars × 256 | 64.3 | 65.3 | 75.3 (1.17×) | n/a |
| write 65,536 chars × 16 | 71.0 | 71.1 | 89.3 (1.26×) | n/a |
| **Text, Unicode, 1 Mi characters, `io.StringIO`** | | | | |
| read whole stream | 666.5 | 670.1 | fails | 1,660.3 (2.49×) |
| `read_chars(64)` until EOF | 1,584.9 | 1,686.8 | n/a | n/a |
| `read_chars(4,096)` until EOF | 711.8 | 716.9 | n/a | n/a |
| `read_chars(65,536)` until EOF | 729.8 | 730.2 | n/a | n/a |
| write 1,048,576 chars × 1 | 745.8 | 744.9 | fails | n/a |
| write 64 chars × 16,384 | 2,057.7 | 2,123.6 | fails | n/a |
| write 4,096 chars × 256 | 849.8 | 852.6 | fails | n/a |
| write 65,536 chars × 16 | 806.9 | 812.0 | fails | n/a |

In summary: construction checks more and costs about 0.3 µs more. Bulk `read_to_end` is 1.3–2.1× faster than either crate, because it appends each returned `bytes` directly instead of zero-filling growing buffers. Whole-stream text reads are 2.2–2.5× faster than `pyo3-filelike`. With 4 KiB or larger chunks, binary I/O is within noise of both crates: Python allocating and copying the payload dominates. With 64-byte chunks it matches `pyo3-file` and is 1.5–2× faster than `pyo3-filelike`. ASCII text writes are 1.14–1.28× faster than `pyo3-file`.

Peak process memory growth for one binary `read_to_end` of 16 MiB (`python pytests/bench.py --memory --size 16777216`, one fresh process per case). Growth is the peak minus the high-water mark before the read, so it includes the 16 MiB result itself. The typed reads ask Python for at most 64 KiB at a time and append each result, rather than doubling a zero-filled buffer:

| Source | typed, bound | typed, detached | `pyo3-file` | `pyo3-filelike` |
|---|---:|---:|---:|---:|
| `io.BytesIO` | +18.0 MiB | +16.6 MiB | +32.5 MiB | +32.4 MiB |
| buffered temporary file | +18.2 MiB | +16.8 MiB | +32.4 MiB | +32.7 MiB |
