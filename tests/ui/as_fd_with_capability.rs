use pyo3_file_typed::{BinaryFileno, BoundBinaryFileno};
use std::os::fd::AsFd;

fn assert_fd<T: AsFd>() {}

fn main() {
    assert_fd::<BinaryFileno>();
    assert_fd::<BoundBinaryFileno<'_>>();
}
