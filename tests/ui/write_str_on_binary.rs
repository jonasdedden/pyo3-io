//! Writing `str` to a binary object would mean choosing an encoding here.
use pyo3_io::PyBinaryWrite;

fn main() {}

fn oops(file: &mut PyBinaryWrite) {
    let _ = file.write_str("text");
}
