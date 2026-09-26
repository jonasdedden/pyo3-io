//! A file that did not ask for `FILENO` has no descriptor to hand out.
use pyo3_io::PyBinaryRead;
use std::os::fd::AsFd;

fn main() {}

fn takes_fd<F: AsFd>(_: F) {}

fn oops(file: PyBinaryRead) {
    takes_fd(file);
}
