use pyo3_file_typed::BinaryRead;

fn cannot_duplicate(file: BinaryRead) {
    let _ = file.try_clone_fd();
}

fn main() {}
