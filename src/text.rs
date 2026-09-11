//! The `str`-shaped API of [`PyFile<Text, ..>`].
//!
//! Python counts a text stream's `read(size)` in characters, so there is no byte count for
//! [`std::io::Read`] to honour and this deliberately does not implement it. Characters cross the
//! boundary as Python Unicode and Rust UTF-8 strings; no assumption is made about the stream's
//! underlying encoding.

use crate::{BoundFile, Error, PyFile, Text};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::PyInt;
use std::borrow::Cow;
use std::io;

/// An opaque, arbitrary-precision position returned by a Python text stream.
///
/// Cookies are meaningful only to the stream that produced them. They are not byte offsets.
#[derive(Debug)]
pub struct TextPosition(Py<PyInt>);

impl TextPosition {
    /// Clones the owned Python reference while attached to Python.
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
    BoundFile<'_, Text, true, WRITE, SEEK, FILENO>
{
    /// Reads at most `count` **characters**, the unit Python's text `read` counts in.
    ///
    /// Returns fewer at end of stream, and an empty string once it is reached.
    pub fn read_chars(&mut self, count: usize) -> io::Result<String> {
        let asked = count;
        let count = i64::try_from(count).map_err(|_| Error::OutOfRange {
            what: "character count",
        })?;
        let text = self.py_read(count)?;
        let got = text.chars().count();
        if got > asked {
            return Err(Error::OverlongTextRead { got, asked }.into());
        }
        Ok(text)
    }

    /// Reads the rest of the stream.
    ///
    /// Uses bounded positive-size reads until EOF, retrying interrupted reads.
    pub fn read_to_string(&mut self) -> io::Result<String> {
        let mut out = String::new();
        loop {
            match self.read_chars(8192) {
                Ok(more) if more.is_empty() => return Ok(out),
                Ok(more) => out.push_str(&more),
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            }
        }
    }

    fn py_read(&self, count: i64) -> Result<String, Error> {
        let obj = self.as_py_object();
        let py = obj.py();
        let res = obj.call_method1(intern!(py, "read"), (count,))?;
        if res.is_none() {
            return Err(Error::would_block_read());
        }
        Ok(res
            .extract::<Cow<'_, str>>()
            .map_err(Error::wrong_payload::<Text>)?
            .into_owned())
    }
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool>
    BoundFile<'_, Text, READ, true, SEEK, FILENO>
{
    /// Writes `text`, returning the number of **characters** written.
    ///
    /// Like [`std::io::Write::write`] this may write less than all of it; see
    /// [`write_all_str`](Self::write_all_str).
    pub fn write_str(&mut self, text: &str) -> io::Result<usize> {
        let obj = self.as_py_object();
        let py = obj.py();
        (|| -> Result<usize, Error> {
            let res = obj.call_method1(intern!(py, "write"), (text,))?;
            if res.is_none() {
                return Err(Error::would_block_write());
            }
            let written = res.extract::<usize>()?;
            let available = text.chars().count();
            if written > available {
                return Err(Error::ImpossibleWriteCount {
                    reported: written,
                    available,
                    unit: "characters",
                });
            }
            Ok(written)
        })()
        .map_err(Into::into)
    }

    /// Writes all of `text`.
    pub fn write_all_str(&mut self, text: &str) -> io::Result<()> {
        let mut rest = text;
        while !rest.is_empty() {
            let written = match self.write_str(rest) {
                Ok(written) => written,
                Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
                Err(err) => return Err(err),
            };
            if written == 0 {
                return Err(Error::WroteNothing { unit: "characters" }.into());
            }
            let consumed = rest
                .char_indices()
                .nth(written)
                .map_or(rest.len(), |(index, _)| index);
            rest = &rest[consumed..];
        }
        Ok(())
    }

    /// Flushes the object, or does nothing if it has no `flush`.
    ///
    /// An object with no `flush` has nothing to flush, so requiring one would turn away every
    /// minimal writer that implements nothing else.
    pub fn flush(&mut self) -> io::Result<()> {
        let obj = self.as_py_object();
        let py = obj.py();
        (|| -> Result<(), Error> {
            let flush = intern!(py, "flush");
            if obj.hasattr(flush)? {
                obj.call_method0(flush)?;
            }
            Ok(())
        })()
        .map_err(Into::into)
    }
}

/// Seeking a text stream.
///
/// There is no [`std::io::Seek`] implementation on purpose. A Python text stream's positions are
/// opaque cookies produced by `tell()`, not byte offsets: the only other things it accepts are the
/// start and the end of the stream. [`SeekFrom::Current`](std::io::SeekFrom::Current) and an
/// arbitrary [`SeekFrom::Start`](std::io::SeekFrom::Start) have no meaning, and Python raises if
/// you try.
impl<const READ: bool, const WRITE: bool, const FILENO: bool>
    BoundFile<'_, Text, READ, WRITE, true, FILENO>
{
    /// The current position, as an opaque cookie for [`seek_to`](Self::seek_to).
    ///
    /// The value is not a byte offset and arithmetic on it is meaningless.
    pub fn tell(&mut self) -> io::Result<TextPosition> {
        let obj = self.as_py_object();
        let py = obj.py();
        (|| -> PyResult<TextPosition> {
            TextPosition::from_python(obj.call_method0(intern!(py, "tell"))?)
        })()
        .map_err(Into::into)
    }

    /// Returns to a position previously reported by [`tell`](Self::tell).
    pub fn seek_to(&mut self, cookie: &TextPosition) -> io::Result<TextPosition> {
        self.py_seek(cookie.0.bind(self.as_py_object().py()), 0)
    }

    /// Returns to the start of the stream.
    pub fn rewind(&mut self) -> io::Result<TextPosition> {
        let zero = 0.into_pyobject(self.as_py_object().py()).unwrap();
        self.py_seek(&zero, 0)
    }

    /// Moves to the end of the stream.
    pub fn seek_to_end(&mut self) -> io::Result<TextPosition> {
        let zero = 0.into_pyobject(self.as_py_object().py()).unwrap();
        self.py_seek(&zero, 2)
    }

    fn py_seek(&self, offset: &Bound<'_, PyInt>, whence: i64) -> io::Result<TextPosition> {
        let obj = self.as_py_object();
        let py = obj.py();
        (|| -> PyResult<TextPosition> {
            let res = obj.call_method1(intern!(py, "seek"), (offset, whence))?;
            TextPosition::from_python(res)
        })()
        .map_err(Into::into)
    }
}

// ---------------------------------------------------------------- detached forms
//
// One attach for the whole operation, then the bound implementation above.

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool>
    PyFile<Text, true, WRITE, SEEK, FILENO>
{
    /// See [`BoundFile::read_chars`].
    pub fn read_chars(&mut self, count: usize) -> io::Result<String> {
        Python::attach(|py| self.bind(py).read_chars(count))
    }

    /// See [`BoundFile::read_to_string`].
    pub fn read_to_string(&mut self) -> io::Result<String> {
        Python::attach(|py| self.bind(py).read_to_string())
    }
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool>
    PyFile<Text, READ, true, SEEK, FILENO>
{
    /// See [`BoundFile::write_str`].
    pub fn write_str(&mut self, text: &str) -> io::Result<usize> {
        Python::attach(|py| self.bind(py).write_str(text))
    }

    /// See [`BoundFile::write_all_str`].
    pub fn write_all_str(&mut self, text: &str) -> io::Result<()> {
        // One attach for the whole retry loop rather than one per short write.
        Python::attach(|py| self.bind(py).write_all_str(text))
    }

    /// See [`BoundFile::flush`].
    pub fn flush(&mut self) -> io::Result<()> {
        Python::attach(|py| self.bind(py).flush())
    }
}

impl<const READ: bool, const WRITE: bool, const FILENO: bool>
    PyFile<Text, READ, WRITE, true, FILENO>
{
    /// See [`BoundFile::tell`].
    pub fn tell(&mut self) -> io::Result<TextPosition> {
        Python::attach(|py| self.bind(py).tell())
    }

    /// See [`BoundFile::seek_to`].
    pub fn seek_to(&mut self, cookie: &TextPosition) -> io::Result<TextPosition> {
        Python::attach(|py| self.bind(py).seek_to(cookie))
    }

    /// See [`BoundFile::rewind`].
    pub fn rewind(&mut self) -> io::Result<TextPosition> {
        Python::attach(|py| self.bind(py).rewind())
    }

    /// See [`BoundFile::seek_to_end`].
    pub fn seek_to_end(&mut self) -> io::Result<TextPosition> {
        Python::attach(|py| self.bind(py).seek_to_end())
    }
}
