// Plain `cargo build` does not add the linker flags maturin would: on macOS the extension must
// leave the interpreter's symbols undefined, to be resolved by the Python process that loads it.
fn main() {
    pyo3_build_config::add_extension_module_link_args();
}
