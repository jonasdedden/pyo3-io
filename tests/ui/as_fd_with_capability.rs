use pyo3_io::{PyBinaryFileno, BoundPyBinaryFileno};
use std::os::fd::AsFd;

fn assert_fd<T: AsFd>() {}

fn main() {
    assert_fd::<PyBinaryFileno>();
    assert_fd::<BoundPyBinaryFileno<'_>>();
}
