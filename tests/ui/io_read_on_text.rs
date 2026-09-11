//! `std::io::Read` yields bytes, which a text object does not have. This is the guarantee a
//! bytes-oriented consumer such as a gzip decoder needs.
use pyo3_typed_io::TextRead;
use std::io::Read;

fn main() {}

fn oops(file: &mut TextRead) {
    let mut buffer = [0u8; 8];
    let _ = file.read(&mut buffer);
}
