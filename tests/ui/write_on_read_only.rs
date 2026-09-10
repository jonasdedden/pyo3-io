//! Writing a read-only file.
use pyo3_file_typed::BinaryRead;
use std::io::Write;

fn main() {}

fn oops(file: &mut BinaryRead) {
    let _ = file.write(b"nope");
}
