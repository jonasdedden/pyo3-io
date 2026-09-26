//! The character API of [`PyTextIO`] and [`BoundPyTextIO`].

use crate::{BoundPyTextIO, Error, PyTextIO, TextPayload};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyInt, PyString};
use std::io;

/// An opaque position returned by a Python text stream's `tell()`, meaningful only to that stream.
#[derive(Debug)]
pub struct PyTextPosition(Py<PyInt>);

/// The number of code points Python stores for `text`, bypassing a `str` subclass's `len()`.
fn char_len(text: &Bound<'_, PyString>) -> usize {
    // SAFETY: `text` is a live, attached `str` (or subclass) instance, for which
    // PyUnicode_GetLength cannot fail; it only reads the stored length.
    let len = unsafe { pyo3::ffi::PyUnicode_GetLength(text.as_ptr()) };
    usize::try_from(len).expect("a str has a non-negative length")
}

impl PyTextPosition {
    /// Clones the position while attached to Python.
    pub fn clone_ref(&self, py: Python<'_>) -> Self {
        Self(self.0.clone_ref(py))
    }

    fn from_python(value: Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self(
            value.cast_into::<PyInt>().map_err(PyErr::from)?.unbind(),
        ))
    }
}

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool>
    BoundPyTextIO<'_, true, WRITE, SEEK, FILENO>
{
    /// Reads at most `count` **characters**, the unit Python's text `read` counts in.
    ///
    /// Returns fewer at end of stream, and an empty string once it is reached.
    pub fn read_chars(&mut self, count: usize) -> io::Result<String> {
        let mut text = String::new();
        self.read_chars_into(count, &mut text)?;
        Ok(text)
    }

    /// Reads the rest of the stream.
    ///
    /// On error, text already consumed is lost; for nonblocking streams, use
    /// [`read_chars`](Self::read_chars) instead.
    pub fn read_to_string(&mut self) -> io::Result<String> {
        let mut out = String::new();
        loop {
            match self.read_chars_into(8192, &mut out) {
                Ok(0) => return Ok(out),
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            }
        }
    }

    /// Appends at most `count` characters to `out`, returning how many; `out` is untouched on
    /// error.
    fn read_chars_into(&self, count: usize, out: &mut String) -> io::Result<usize> {
        let size = i64::try_from(count).map_err(|_| Error::OutOfRange {
            what: "character count",
        })?;
        let obj = self.as_py_object();
        let py = obj.py();
        let res = obj.call_method1(intern!(py, "read"), (size,))?;
        if res.is_none() {
            return Err(Error::would_block_read().into());
        }
        let text = res
            .cast::<PyString>()
            .map_err(|err| Error::wrong_payload::<TextPayload>(err.into()))?;
        // Borrows the UTF-8 form CPython caches on the string.
        let utf8 = text.to_cow().map_err(Error::wrong_payload::<TextPayload>)?;
        let got = char_len(text);
        if got > count {
            return Err(Error::OverlongTextRead { got, asked: count }.into());
        }
        out.push_str(&utf8);
        Ok(got)
    }
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool>
    BoundPyTextIO<'_, READ, true, SEEK, FILENO>
{
    /// Writes `text`, returning the number of **characters** written.
    ///
    /// Like [`std::io::Write::write`] this may write less than all of it; see
    /// [`write_all_str`](Self::write_all_str).
    pub fn write_str(&mut self, text: &str) -> io::Result<usize> {
        self.py_write(text).map(|(written, _)| written)
    }

    /// Writes all of `text`.
    pub fn write_all_str(&mut self, text: &str) -> io::Result<()> {
        let mut rest = text;
        while !rest.is_empty() {
            let (written, available) = match self.py_write(rest) {
                Ok(counts) => counts,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            };
            if written == 0 {
                return Err(Error::WroteNothing { unit: "characters" }.into());
            }
            if written == available {
                return Ok(());
            }
            // A short write: only now is it worth walking the characters to find where to resume.
            let consumed = rest
                .char_indices()
                .nth(written)
                .map_or(rest.len(), |(index, _)| index);
            rest = &rest[consumed..];
        }
        Ok(())
    }

    /// One `write` call, returning the characters written and the characters offered.
    fn py_write(&self, text: &str) -> io::Result<(usize, usize)> {
        let obj = self.as_py_object();
        let py = obj.py();
        let text = PyString::new(py, text);
        // Python already knows the length; counting in Rust would be a second pass.
        let available = char_len(&text);
        let written = crate::write::write_result(
            py,
            obj.call_method1(intern!(py, "write"), (text,)),
            available,
            "characters",
        )?;
        Ok((written, available))
    }

    /// Flushes the object, or does nothing if it has no `flush`.
    pub fn flush(&mut self) -> io::Result<()> {
        crate::write::flush(self.as_py_object())
    }
}

/// Text streams seek only to cookies from `tell()`, the start or the end, so there is no
/// [`std::io::Seek`].
impl<'py, const READ: bool, const WRITE: bool, const FILENO: bool>
    BoundPyTextIO<'py, READ, WRITE, true, FILENO>
{
    /// The current position, as an opaque cookie for [`seek_to`](Self::seek_to).
    pub fn tell(&mut self) -> io::Result<PyTextPosition> {
        let obj = self.as_py_object();
        let py = obj.py();
        Ok(PyTextPosition::from_python(
            obj.call_method0(intern!(py, "tell"))?,
        )?)
    }

    /// Returns to a position previously reported by [`tell`](Self::tell).
    pub fn seek_to(&mut self, cookie: &PyTextPosition) -> io::Result<PyTextPosition> {
        self.py_seek(cookie.0.bind(self.as_py_object().py()), 0)
    }

    /// Returns to the start of the stream.
    pub fn rewind(&mut self) -> io::Result<PyTextPosition> {
        self.py_seek(0, 0)
    }

    /// Moves to the end of the stream.
    pub fn seek_to_end(&mut self) -> io::Result<PyTextPosition> {
        self.py_seek(0, 2)
    }

    fn py_seek(&self, offset: impl IntoPyObject<'py>, whence: i64) -> io::Result<PyTextPosition> {
        let obj = self.as_py_object();
        let py = obj.py();
        let res = obj.call_method1(intern!(py, "seek"), (offset, whence))?;
        Ok(PyTextPosition::from_python(res)?)
    }
}

// ---------------------------------------------------------------- detached forms
//
// Attach once and delegate to the bound implementation above.

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool> PyTextIO<true, WRITE, SEEK, FILENO> {
    /// See [`BoundPyTextIO::read_chars`](crate::BoundPyTextIO#method.read_chars).
    pub fn read_chars(&mut self, count: usize) -> io::Result<String> {
        Python::attach(|py| self.bind(py).read_chars(count))
    }

    /// See [`BoundPyTextIO::read_to_string`](crate::BoundPyTextIO#method.read_to_string).
    pub fn read_to_string(&mut self) -> io::Result<String> {
        Python::attach(|py| self.bind(py).read_to_string())
    }
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool> PyTextIO<READ, true, SEEK, FILENO> {
    /// See [`BoundPyTextIO::write_str`](crate::BoundPyTextIO#method.write_str).
    pub fn write_str(&mut self, text: &str) -> io::Result<usize> {
        Python::attach(|py| self.bind(py).write_str(text))
    }

    /// See [`BoundPyTextIO::write_all_str`](crate::BoundPyTextIO#method.write_all_str).
    pub fn write_all_str(&mut self, text: &str) -> io::Result<()> {
        Python::attach(|py| self.bind(py).write_all_str(text))
    }

    /// See [`BoundPyTextIO::flush`](crate::BoundPyTextIO#method.flush).
    pub fn flush(&mut self) -> io::Result<()> {
        Python::attach(|py| self.bind(py).flush())
    }
}

impl<const READ: bool, const WRITE: bool, const FILENO: bool> PyTextIO<READ, WRITE, true, FILENO> {
    /// See [`BoundPyTextIO::tell`](crate::BoundPyTextIO#method.tell).
    pub fn tell(&mut self) -> io::Result<PyTextPosition> {
        Python::attach(|py| self.bind(py).tell())
    }

    /// See [`BoundPyTextIO::seek_to`](crate::BoundPyTextIO#method.seek_to).
    pub fn seek_to(&mut self, cookie: &PyTextPosition) -> io::Result<PyTextPosition> {
        Python::attach(|py| self.bind(py).seek_to(cookie))
    }

    /// See [`BoundPyTextIO::rewind`](crate::BoundPyTextIO#method.rewind).
    pub fn rewind(&mut self) -> io::Result<PyTextPosition> {
        Python::attach(|py| self.bind(py).rewind())
    }

    /// See [`BoundPyTextIO::seek_to_end`](crate::BoundPyTextIO#method.seek_to_end).
    pub fn seek_to_end(&mut self) -> io::Result<PyTextPosition> {
        Python::attach(|py| self.bind(py).seek_to_end())
    }
}
