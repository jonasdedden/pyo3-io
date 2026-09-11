#!/usr/bin/env bash
# Standard checks need only published dependencies; custom stub generation is opt-in.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate="$(dirname "$here")"
stubs=0
bench=0
for arg in "$@"; do
    case "$arg" in
        --stubs) stubs=1 ;;
        --bench) bench=1 ;;
        --help)
            echo "Usage: $0 [--stubs] [--bench]"
            echo "--stubs requires PYO3_INTROSPECTION or PYO3_CHECKOUT with attach_to_root support."
            exit 0
            ;;
        *) echo "Unknown argument: $arg" >&2; exit 2 ;;
    esac
done
if (( stubs )) && [[ -z "${PYO3_INTROSPECTION:-}" && -z "${PYO3_CHECKOUT:-}" ]]; then
    echo "--stubs requires an explicit PYO3_INTROSPECTION executable or PYO3_CHECKOUT." >&2
    echo "The generator must support attach_to_root; ordinary PyO3 builds need no patch." >&2
    exit 2
fi
export PYTESTS_PROFILE=release
export PYTESTS_STUBS="$stubs"

step() { printf '\n\033[1m== %s\033[0m\n' "$1"; }

step "cargo fmt / clippy"
cargo fmt --manifest-path "$crate/Cargo.toml" --check
cargo fmt --manifest-path "$here/Cargo.toml" --check
cargo clippy --manifest-path "$crate/Cargo.toml" --all-targets -- -D warnings
cargo clippy --manifest-path "$crate/Cargo.toml" --all-features --all-targets -- -D warnings
cargo clippy --manifest-path "$here/Cargo.toml" --all-targets -- -D warnings

step "cargo test: default and all features"
cargo test --manifest-path "$crate/Cargo.toml"
cargo test --manifest-path "$crate/Cargo.toml" --all-features

step "rustdoc: rendered API surface and links"
python3 "$here/check_rustdoc.py"

step "build the release test extension"
artifact="$(python3 "$here/python/_extension.py" --profile release)"

if (( stubs )); then
    step "generate type stubs"
    output="$here/typecheck/pyo3_typed_io_tests-stubs"
    if [[ -n "${PYO3_INTROSPECTION:-}" ]]; then
        "$PYO3_INTROSPECTION" "$artifact" pyo3_typed_io_tests "$output"
    else
        cargo run --locked --quiet -p pyo3-introspection \
            --manifest-path "$PYO3_CHECKOUT/Cargo.toml" \
            --target-dir "$here/target/introspection" -- \
            "$artifact" pyo3_typed_io_tests "$output"
    fi
fi

step "pytest: runtime behaviour, comparisons, stub snapshot"
(cd "$here/python" && uvx --with pytest pytest -q)

if (( stubs )); then
    step "pyright: accepted and rejected contracts"
    (cd "$here/typecheck" && pyright check_ok.py)
    report=$(cd "$here/typecheck" && pyright --outputjson check_bad.py || true)
    # Check the locations as well as the count: an unrelated error must not mask a lost rule.
    printf '%s' "$report" | python3 "$here/typecheck/check_diagnostics.py"
fi

if (( bench )); then
    step "release benchmark against published comparison crates"
    python3 "$here/bench.py" --quick
fi

printf '\n\033[1;32mall good\033[0m\n'
