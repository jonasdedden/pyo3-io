//! Characters are the text API's unit; a binary object counts bytes.
use pyo3_io::PyBinaryRead;

fn main() {}

fn oops(file: &mut PyBinaryRead) {
    let _ = file.read_chars(4);
}
