#!/usr/bin/env bash
# Everything: Rust tests, stub generation, runtime tests, type-checker tests, benchmark.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(dirname "$here")"
pyo3="${PYO3_CHECKOUT:-$(dirname "$crate")/pyo3}"

step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

step "cargo fmt / clippy"
cargo fmt --manifest-path "$crate/Cargo.toml" --check
cargo clippy --manifest-path "$crate/Cargo.toml" --features experimental-inspect --all-targets -- -D warnings

step "cargo test: compile-fail and API surface"
cargo test --manifest-path "$crate/Cargo.toml" --features experimental-inspect

step "build the test extension"
cargo build --manifest-path "$here/Cargo.toml"

step "generate type stubs"
# The `attach_to_root` support lives in pyo3-introspection, not in the pyo3 crate the extension
# compiles against, so the generator has to come from that checkout.
cargo run --quiet -p pyo3-introspection --manifest-path "$pyo3/Cargo.toml" -- \
    "$here/target/debug/libpyo3_file_typed_tests.so" \
    pyo3_file_typed_tests \
    "$here/typecheck/pyo3_file_typed_tests-stubs"

step "pytest: runtime behaviour, comparisons, stub snapshot"
(cd "$here/python" && uvx --with pytest pytest -q)

step "pyright: check_ok.py must have 0 errors"
(cd "$here/typecheck" && pyright check_ok.py 2>/dev/null)

step "pyright: check_bad.py must have 10 errors, one per rule"
# pyright exits non-zero when it finds errors, which is the point here.
report=$(cd "$here/typecheck" && pyright --outputjson check_bad.py 2>/dev/null || true)
count=$(printf '%s' "$report" | python3 -c 'import json,sys; print(json.load(sys.stdin)["summary"]["errorCount"])')
if [ "$count" != "10" ]; then
    echo "expected 10 errors from check_bad.py, got $count" >&2
    exit 1
fi
echo "10 errors, as expected"

step "benchmark against pyo3-file"
(cd "$here" && uvx --with pytest python bench.py)

printf '\n\033[1;32mall good\033[0m\n'
