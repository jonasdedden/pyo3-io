//! The bound form cannot leave the thread; that is what the detached form is for.
use pyo3_typed_io::BoundBinaryRead;

fn main() {}

fn assert_send<T: Send>(_: T) {}

fn oops(file: BoundBinaryRead<'_>) {
    assert_send(file);
}
