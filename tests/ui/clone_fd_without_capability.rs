use pyo3_typed_io::PyBinaryRead;

fn cannot_duplicate(file: PyBinaryRead) {
    let _ = file.try_clone_fd();
}

fn main() {}
