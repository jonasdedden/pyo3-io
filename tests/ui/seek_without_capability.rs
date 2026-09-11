//! Seeking a file that was not asked to be seekable.
use pyo3_typed_io::BinaryRead;
use std::io::{Seek, SeekFrom};

fn main() {}

fn oops(file: &mut BinaryRead) {
    let _ = file.seek(SeekFrom::Start(0));
}
