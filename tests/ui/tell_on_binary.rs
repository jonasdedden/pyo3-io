//! `tell` is the text cookie API; a binary file uses `io::Seek::stream_position`.
use pyo3_file_typed::BinaryReadSeek;

fn main() {}

fn oops(file: &mut BinaryReadSeek) {
    let _ = file.tell();
}
