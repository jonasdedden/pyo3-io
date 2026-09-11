//! A partial BlockingIOError is progress, not permission to resend the same input.

use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3_file_typed::{BinaryWrite, TextWrite};
use std::ffi::CString;
use std::io::{ErrorKind, Write};

fn fixture<'py>(py: Python<'py>, source: &str) -> Bound<'py, PyModule> {
    PyModule::from_code(py, &CString::new(source).unwrap(), c"write.py", c"write").unwrap()
}

#[test]
fn buffered_writer_reports_partial_progress_before_blocking() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(
            py,
            r#"
import io
class Raw(io.RawIOBase):
    blocked = True
    data = b""
    def writable(self): return True
    def write(self, data):
        if self.blocked: return None
        self.data += bytes(data)
        return len(data)
raw = Raw()
stream = io.BufferedWriter(raw, buffer_size=4)
"#,
        );
        let mut file = BinaryWrite::py_new(module.getattr("stream").unwrap()).unwrap();
        assert_eq!(file.write(b"abcdef").unwrap(), 4);
        assert_eq!(file.write(b"ef").unwrap_err().kind(), ErrorKind::WouldBlock);
        module
            .getattr("raw")
            .unwrap()
            .setattr("blocked", false)
            .unwrap();
        file.flush().unwrap();
        file.write_all(b"ef").unwrap();
        file.flush().unwrap();
        let data: Vec<u8> = module
            .getattr("raw")
            .unwrap()
            .getattr("data")
            .unwrap()
            .extract()
            .unwrap();
        assert_eq!(data, b"abcdef");
    });
}

#[test]
fn text_progress_is_counted_in_characters_not_utf8_bytes() {
    Python::initialize();
    Python::attach(|py| {
        let module = fixture(
            py,
            r#"
class Stream:
    data = ""
    blocked = False
    def write(self, text):
        if not self.blocked:
            self.data += text[:1]
            self.blocked = True
            raise BlockingIOError(11, "blocked", 1)
        self.data += text
        return len(text)
stream = Stream()
"#,
        );
        let object = module.getattr("stream").unwrap();
        let mut file = TextWrite::py_new(object.clone()).unwrap();
        file.write_all_str("é🦀").unwrap();
        assert_eq!(
            object.getattr("data").unwrap().extract::<String>().unwrap(),
            "é🦀"
        );
    });
}

#[test]
fn blocking_counts_are_checked_and_missing_progress_stays_would_block() {
    Python::initialize();
    Python::attach(|py| {
        for (args, expected) in [
            ("11, 'blocked'", ErrorKind::WouldBlock),
            ("11, 'blocked', 0", ErrorKind::WouldBlock),
            ("11, 'blocked', 5", ErrorKind::InvalidData),
        ] {
            let module = fixture(py, &format!(
                "class Stream:\n def write(self, data): raise BlockingIOError({args})\nstream = Stream()"
            ));
            let object = module.getattr("stream").unwrap();
            assert_eq!(
                BinaryWrite::py_new(object.clone())
                    .unwrap()
                    .write(b"x")
                    .unwrap_err()
                    .kind(),
                expected
            );
            assert_eq!(
                TextWrite::py_new(object)
                    .unwrap()
                    .write_str("é")
                    .unwrap_err()
                    .kind(),
                expected
            );
        }
        for count in ["-1", "'not a count'"] {
            let module = fixture(py, &format!(
                "class Stream:\n def write(self, data):\n  err = BlockingIOError(11, 'blocked')\n  err.characters_written = {count}\n  raise err\nstream = Stream()"
            ));
            let object = module.getattr("stream").unwrap();
            assert!(BinaryWrite::py_new(object.clone())
                .unwrap()
                .write(b"x")
                .is_err());
            assert!(TextWrite::py_new(object).unwrap().write_str("é").is_err());
        }
    });
}
