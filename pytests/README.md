# Validation and benchmarks

Run from the repository root with Rust, Python, and `uv` available:

```sh
cargo test --doc
python3 pytests/check_rustdoc.py
./pytests/run-tests.sh
```

The runner defaults to standard Rust and Python validation, without a custom stub generator, and checks freshly rendered rustdoc pages and their links. Opt in to stub generation/type checking or benchmarks:

```sh
PYO3_INTROSPECTION=/path/to/pyo3-introspection ./pytests/run-tests.sh --stubs
# Alternatively supply PYO3_CHECKOUT=/path/to/custom/pyo3 for the generator.
./pytests/run-tests.sh --bench
python pytests/bench.py --quick
python pytests/bench.py --profile release
python pytests/bench.py --memory --size 8388608
```

`--stubs` requires the custom generator explicitly; there is no neighboring-checkout default. Stub checking also needs `pyright`. The Python benchmark defaults to a release build (built incrementally outside timing) and reports median and spread for constructor-only, chunk, bulk, real-file, and ASCII/Unicode text read and write workloads; `--filter TEXT` runs only the workloads whose label contains `TEXT`. Use its results for the measured workload, not as a fixed speed claim. `--memory` runs each binary bulk-read case in a fresh subprocess and reports peak process RSS on Linux/macOS. It includes Python, extension loading and input setup; it is not an allocated-byte count or a claim about the adapter alone.
