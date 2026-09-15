//! Descriptor duplication owns a new lifetime rather than borrowing Python's descriptor.
#![cfg(unix)]

use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3_typed_io::PyBinaryFileno;
use std::io::Read;
use std::os::fd::AsRawFd;

#[test]
fn owned_duplicate_survives_python_close() {
    Python::initialize();
    Python::attach(|py| {
        let source = py
            .import("tempfile")
            .unwrap()
            .call_method0("TemporaryFile")
            .unwrap();
        source.call_method1("write", (b"abc".as_slice(),)).unwrap();
        source.call_method0("flush").unwrap();
        source.call_method1("seek", (0,)).unwrap();
        let file = PyBinaryFileno::py_new(source.clone()).unwrap();
        let duplicate = file.try_clone_fd().unwrap();
        let bound_duplicate = file.bind(py).try_clone_fd().unwrap();
        assert_ne!(duplicate.as_raw_fd(), file.fileno().unwrap());
        // SAFETY: F_GETFD only reads descriptor flags; duplicate is owned and remains open.
        let flags = unsafe { libc::fcntl(duplicate.as_raw_fd(), libc::F_GETFD) };
        assert_ne!(flags & libc::FD_CLOEXEC, 0);
        source.call_method0("close").unwrap();
        assert!(file.fileno().is_err());
        let mut owned_file = std::fs::File::from(duplicate);
        let mut data = String::new();
        owned_file.read_to_string(&mut data).unwrap();
        assert_eq!(data, "abc");
        // Duplicates share the same file offset, not Python's buffering layer.
        let mut other = std::fs::File::from(bound_duplicate);
        assert_eq!(other.read(&mut [0; 1]).unwrap(), 0);
    });
}

#[test]
fn unavailable_and_invalid_descriptors_are_errors() {
    Python::initialize();
    Python::attach(|py| {
        let memory = py.import("io").unwrap().call_method0("BytesIO").unwrap();
        assert!(PyBinaryFileno::py_new(memory)
            .unwrap()
            .try_clone_fd()
            .is_err());
        let module = PyModule::from_code(
            py,
            c"class Invalid:\n    def fileno(self): return -1\n",
            c"fd_contract.py",
            c"fd_contract",
        )
        .unwrap();
        let invalid =
            PyBinaryFileno::py_new(module.getattr("Invalid").unwrap().call0().unwrap()).unwrap();
        assert_eq!(
            invalid.try_clone_fd().unwrap_err().raw_os_error(),
            Some(libc::EBADF)
        );
    });
}
