//! `tell` is the text cookie API; a binary file uses `io::Seek::stream_position`.
use pyo3_typed_io::PyBinaryReadSeek;

fn main() {}

fn oops(file: &mut PyBinaryReadSeek) {
    let _ = file.tell();
}
