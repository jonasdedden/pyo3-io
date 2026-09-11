//! Asking for a file descriptor that was not required.
use pyo3_typed_io::BinaryRead;

fn main() {}

fn oops(file: &BinaryRead) {
    let _ = file.fileno();
}
