//! Reading a write-only file.
use pyo3_file_typed::BinaryWrite;
use std::io::Read;

fn main() {}

fn oops(file: &mut BinaryWrite) {
    let mut buffer = [0u8; 8];
    let _ = file.read(&mut buffer);
}
