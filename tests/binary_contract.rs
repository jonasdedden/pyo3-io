//! Binary readers follow the Python buffer protocol and Rust's `Read` contract.

use pyo3::{prelude::*, types::PyModule};
use pyo3_typed_io::BinaryRead;
use std::{
    ffi::CString,
    io::{self, Read},
};

fn reader(events: &str) -> BinaryRead {
    Python::initialize();
    Python::attach(|py| {
        let code = CString::new(format!(
            r#"
from array import array
class Reader:
    mode = "rb"
    def __init__(self):
        self.events = iter({events})
    def read(self, size):
        assert size > 0, "only positive bounded reads are supported"
        value = next(self.events, b"")
        if isinstance(value, BaseException):
            raise value
        return value
reader = Reader()
"#
        ))
        .unwrap();
        let module = PyModule::from_code(
            py,
            &code,
            pyo3::ffi::c_str!("contract.py"),
            pyo3::ffi::c_str!("contract"),
        )
        .unwrap();
        BinaryRead::py_new(module.getattr("reader").unwrap()).unwrap()
    })
}

#[test]
fn buffers_are_raw_bytes_not_integer_sequences() {
    Python::initialize();
    for expression in [
        "b'abc'",
        "bytearray(b'abc')",
        "memoryview(b'abcdef')[::2]",
        "memoryview(b'abcdef')[::-1]",
        "array('I', [1, 2])",
        "memoryview(array('I', [1, 2]))",
        "memoryview(array('I', [1, 2, 3, 4]))[::2]",
        "memoryview(b'abcdef').cast('B', shape=[2, 3])",
    ] {
        let expected = Python::attach(|py| {
            let code = CString::new(format!("memoryview({expression}).tobytes()")).unwrap();
            let locals = pyo3::types::PyDict::new(py);
            locals
                .set_item(
                    "array",
                    py.import("array").unwrap().getattr("array").unwrap(),
                )
                .unwrap();
            py.eval(&code, None, Some(&locals))
                .unwrap()
                .extract::<Vec<u8>>()
                .unwrap()
        });
        let mut file = reader(&format!("[{expression}]"));
        let mut out = [0; 64];
        let count = file.read(&mut out).unwrap();
        assert_eq!(&out[..count], expected, "{expression}");
    }
}

#[test]
fn non_buffers_and_oversized_results_leave_destination_untouched() {
    for events in ["[[1, 2]]", "[b'abc']", "[memoryview(array('I', [1, 2]))]"] {
        let mut file = reader(events);
        let mut out = [99; 2];
        assert_eq!(
            file.read(&mut out).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(out, [99; 2]);
    }
}

#[test]
fn bulk_reads_use_positive_sizes_and_retry_interruptions() {
    let events = "[InterruptedError(), b'a', InterruptedError(), b'bc']";
    let mut file = reader(events);
    let mut out = b"prefix".to_vec();
    assert_eq!(file.read_to_end(&mut out).unwrap(), 3);
    assert_eq!(out, b"prefixabc");

    let mut file = reader(events);
    let mut out = [0; 3];
    file.read_exact(&mut out).unwrap();
    assert_eq!(&out, b"abc");

    let mut file = reader(events);
    let mut out = String::from("prefix");
    assert_eq!(file.read_to_string(&mut out).unwrap(), 3);
    assert_eq!(out, "prefixabc");
}

#[test]
fn read_to_end_keeps_partial_data_on_error() {
    let mut file = reader("[b'abc', BlockingIOError()]");
    let mut out = b"prefix".to_vec();
    assert_eq!(
        file.read_to_end(&mut out).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(out, b"prefixabc");
}

#[test]
fn read_to_string_uses_std_partial_error_and_utf8_semantics() {
    for (events, kind, expected) in [
        (
            "[b'abc', BlockingIOError()]",
            io::ErrorKind::WouldBlock,
            "prefixabc",
        ),
        ("[b'abc', b'\\xff']", io::ErrorKind::InvalidData, "prefix"),
        (
            "[b'abc', b'\\xff', BlockingIOError()]",
            io::ErrorKind::WouldBlock,
            "prefix",
        ),
    ] {
        let mut file = reader(events);
        let mut out = String::from("prefix");
        assert_eq!(file.read_to_string(&mut out).unwrap_err().kind(), kind);
        assert_eq!(out, expected);
    }
}
