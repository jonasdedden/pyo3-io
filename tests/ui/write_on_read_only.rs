//! Writing a read-only file.
use pyo3_typed_io::PyBinaryRead;
use std::io::Write;

fn main() {}

fn oops(file: &mut PyBinaryRead) {
    let _ = file.write(b"nope");
}
