//! Characters are the text API's unit; a binary object counts bytes.
use pyo3_typed_io::BinaryRead;

fn main() {}

fn oops(file: &mut BinaryRead) {
    let _ = file.read_chars(4);
}
