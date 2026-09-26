//! Asking for a file descriptor that was not required.
use pyo3_io::PyBinaryRead;

fn main() {}

fn oops(file: &PyBinaryRead) {
    let _ = file.fileno();
}
