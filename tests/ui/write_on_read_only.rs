//! Writing a read-only file.
use pyo3_typed_io::BinaryRead;
use std::io::Write;

fn main() {}

fn oops(file: &mut BinaryRead) {
    let _ = file.write(b"nope");
}
