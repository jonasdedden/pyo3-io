//! Reading a write-only file.
use pyo3_typed_io::PyBinaryWrite;
use std::io::Read;

fn main() {}

fn oops(file: &mut PyBinaryWrite) {
    let mut buffer = [0u8; 8];
    let _ = file.read(&mut buffer);
}
