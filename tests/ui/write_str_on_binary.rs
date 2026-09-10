//! Writing `str` to a binary object would mean choosing an encoding here.
use pyo3_file_typed::BinaryWrite;

fn main() {}

fn oops(file: &mut BinaryWrite) {
    let _ = file.write_str("text");
}
