use pyo3_typed_io::BinaryRead;

fn cannot_duplicate(file: BinaryRead) {
    let _ = file.try_clone_fd();
}

fn main() {}
