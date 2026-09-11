//! A text stream's positions are opaque cookies, not byte offsets, so there is no `io::Seek`.
use pyo3_typed_io::TextReadSeek;
use std::io::{Seek, SeekFrom};

fn main() {}

fn oops(file: &mut TextReadSeek) {
    let _ = file.seek(SeekFrom::Current(4));
}
