//! `std::io::Write` takes bytes, which a text object does not accept.
use pyo3_file_typed::TextWrite;
use std::io::Write;

fn main() {}

fn oops(file: &mut TextWrite) {
    let _ = file.write(b"bytes");
}
