//! `std::io::Write` takes bytes, which a text object does not accept.
use pyo3_typed_io::PyTextWrite;
use std::io::Write;

fn main() {}

fn oops(file: &mut PyTextWrite) {
    let _ = file.write(b"bytes");
}
