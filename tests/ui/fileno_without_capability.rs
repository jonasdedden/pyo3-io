//! Asking for a file descriptor that was not required.
use pyo3_file_typed::BinaryRead;

fn main() {}

fn oops(file: &BinaryRead) {
    let _ = file.fileno();
}
