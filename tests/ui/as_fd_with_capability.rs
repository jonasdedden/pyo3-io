use pyo3_typed_io::{BinaryFileno, BoundBinaryFileno};
use std::os::fd::AsFd;

fn assert_fd<T: AsFd>() {}

fn main() {
    assert_fd::<BinaryFileno>();
    assert_fd::<BoundBinaryFileno<'_>>();
}
