//! Seeking a file that was not asked to be seekable.
use pyo3_typed_io::PyBinaryRead;
use std::io::{Seek, SeekFrom};

fn main() {}

fn oops(file: &mut PyBinaryRead) {
    let _ = file.seek(SeekFrom::Start(0));
}
