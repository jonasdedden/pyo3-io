//! Text streams count characters and carry opaque Python positions.
use pyo3::prelude::*;
use pyo3::types::PyModule;
use pyo3_typed_io::{TextRead, TextReadSeek, TextSeek, TextWrite};
use std::ffi::CString;
use std::io::ErrorKind;

fn fixture<'py>(py: Python<'py>, source: &str) -> Bound<'py, PyAny> {
    PyModule::from_code(
        py,
        &CString::new(source).unwrap(),
        c"fixture.py",
        c"fixture",
    )
    .unwrap()
    .getattr("stream")
    .unwrap()
}

#[test]
fn newline_decoder_cookie_round_trips_without_narrowing() {
    Python::initialize();
    let (mut file, cookie) = Python::attach(|py| {
        let obj = fixture(py, "import io\nstream = io.TextIOWrapper(io.BytesIO(b'a\\rb'), encoding='utf8', newline=None)");
        let file: TextReadSeek = obj.extract().unwrap();
        assert_eq!(file.bind(py).read_chars(2).unwrap(), "a\n");
        assert_eq!(
            obj.call_method0("tell")
                .unwrap()
                .call_method0("bit_length")
                .unwrap()
                .extract::<usize>()
                .unwrap(),
            129
        );
        let cookie = file.bind(py).tell().unwrap().clone_ref(py);
        (file, cookie)
    });
    assert_eq!(file.read_to_string().unwrap(), "b");
    let returned = file.seek_to(&cookie).unwrap();
    assert_eq!(file.read_chars(1).unwrap(), "b");
    file.seek_to(&returned).unwrap();
    assert_eq!(file.read_to_string().unwrap(), "b");
    file.rewind().unwrap();
    assert_eq!(file.read_chars(1).unwrap(), "a");
    file.seek_to_end().unwrap();
    assert_eq!(file.read_chars(1).unwrap(), "");
}

#[test]
fn cookies_require_python_integers_but_impose_no_sign_limit() {
    Python::initialize();
    Python::attach(|py| {
        let obj = fixture(py, "class Stream:\n def tell(self): return -2**200\n def seek(self, cookie, whence):\n  assert cookie == -2**200\n  return cookie\nstream = Stream()");
        let mut file: TextSeek = obj.extract().unwrap();
        let cookie = file.tell().unwrap();
        file.seek_to(&cookie).unwrap();
        for value in [
            "'12'",
            "1.5",
            "None",
            "type('Index', (), {'__index__': lambda self: 12})()",
        ] {
            let obj = fixture(py, &format!("class Stream:\n def tell(self): return {value}\n def seek(self, cookie, whence): return {value}\nstream = Stream()"));
            let mut file: TextSeek = obj.extract().unwrap();
            assert!(file.tell().is_err(), "{value}");
            assert!(file.rewind().is_err(), "{value}");
        }
    });
}

#[test]
fn read_limits_are_unicode_characters_including_zero() {
    Python::initialize();
    Python::attach(|py| {
        let obj = fixture(py, "import io\nstream = io.StringIO('é🦀z')");
        let mut file: TextRead = obj.extract().unwrap();
        assert_eq!(file.read_chars(0).unwrap(), "");
        assert_eq!(file.read_chars(2).unwrap(), "é🦀");
        assert_eq!(file.read_chars(2).unwrap(), "z");
        for count in [0, 1] {
            let obj = fixture(
                py,
                "class Stream:\n def read(self, size): return 'é🦀'\nstream = Stream()",
            );
            let mut file: TextRead = obj.extract().unwrap();
            let err = file.read_chars(count).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidData);
            assert!(err.to_string().contains("2 characters"));
        }
    });
}

#[test]
fn bulk_reads_use_positive_counts_and_retry_interruptions() {
    Python::initialize();
    Python::attach(|py| {
        let obj = fixture(py, "class Stream:\n def __init__(self): self.steps = iter([None, 'é', None, '🦀', ''])\n def read(self, size):\n  assert 0 < size <= 8192\n  value = next(self.steps)\n  if value is None: raise InterruptedError()\n  return value\nstream = Stream()");
        let mut file: TextRead = obj.extract().unwrap();
        assert_eq!(file.read_to_string().unwrap(), "é🦀");
        let obj = fixture(
            py,
            "class Stream:\n def read(self, size): return 'é' * (size + 1)\nstream = Stream()",
        );
        let mut file: TextRead = obj.extract().unwrap();
        assert_eq!(
            file.read_to_string().unwrap_err().kind(),
            ErrorKind::InvalidData
        );
    });
}

#[test]
fn bulk_writes_retry_without_losing_multibyte_characters_and_flush_is_optional() {
    Python::initialize();
    Python::attach(|py| {
        let obj = fixture(py, "class Stream:\n def __init__(self): self.calls = 0; self.text = ''\n def write(self, text):\n  self.calls += 1\n  if self.calls % 2: raise InterruptedError()\n  self.text += text[0]\n  return 1\nstream = Stream()");
        let mut file: TextWrite = obj.extract().unwrap();
        file.write_all_str("é🦀z").unwrap();
        file.flush().unwrap();
        assert_eq!(
            obj.getattr("text").unwrap().extract::<String>().unwrap(),
            "é🦀z"
        );
    });
}
