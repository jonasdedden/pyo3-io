//! The `str`-shaped API of [`PyFile<Text, ..>`].
//!
//! Python counts a text stream's `read(size)` in characters, so there is no byte count for
//! [`std::io::Read`] to honour and this deliberately does not implement it. Characters cross the
//! boundary as characters: nothing is encoded to UTF-8 on the way out and decoded again on the way
//! back, and no assumption is made about the object's encoding.

use crate::{BoundFile, Error, PyFile, Text};
use pyo3::intern;
use pyo3::prelude::*;
use std::borrow::Cow;
use std::io;

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool>
    BoundFile<'_, Text, true, WRITE, SEEK, FILENO>
{
    /// Reads at most `count` **characters**, the unit Python's text `read` counts in.
    ///
    /// Returns fewer at end of stream, and an empty string once it is reached.
    pub fn read_chars(&mut self, count: usize) -> io::Result<String> {
        let count = i64::try_from(count).map_err(|_| Error::OutOfRange {
            what: "character count",
        })?;
        Ok(self.py_read(count)?)
    }

    /// Reads the rest of the stream.
    ///
    /// This is a single `read(-1)` and a single `str` handed across the boundary. Reading the same
    /// data as bytes would mean encoding it to UTF-8 on the Python side and validating it again on
    /// the Rust side, for a result that is not the file's bytes anyway unless it happened to be
    /// UTF-8 encoded.
    pub fn read_to_string(&mut self) -> io::Result<String> {
        let read = || -> Result<String, Error> {
            let mut out = self.py_read(-1)?;
            // `read(-1)` may hand back a partial result, so keep asking until it is empty
            loop {
                let more = self.py_read(-1)?;
                if more.is_empty() {
                    return Ok(out);
                }
                out.push_str(&more);
            }
        };
        read().map_err(Into::into)
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
            let written = self.write_str(rest)?;
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
    pub fn tell(&mut self) -> io::Result<u64> {
        let obj = self.as_py_object();
        let py = obj.py();
        (|| -> Result<u64, Error> { Ok(obj.call_method0(intern!(py, "tell"))?.extract::<u64>()?) })(
        )
        .map_err(Into::into)
    }

    /// Returns to a position previously reported by [`tell`](Self::tell).
    pub fn seek_to(&mut self, cookie: u64) -> io::Result<u64> {
        let cookie = i64::try_from(cookie).map_err(|_| Error::OutOfRange {
            what: "position cookie",
        })?;
        self.py_seek(cookie, 0)
    }

    /// Returns to the start of the stream.
    pub fn rewind(&mut self) -> io::Result<u64> {
        self.py_seek(0, 0)
    }

    /// Moves to the end of the stream.
    pub fn seek_to_end(&mut self) -> io::Result<u64> {
        self.py_seek(0, 2)
    }

    fn py_seek(&self, offset: i64, whence: i64) -> io::Result<u64> {
        let obj = self.as_py_object();
        let py = obj.py();
        (|| -> Result<u64, Error> {
            let res = obj.call_method1(intern!(py, "seek"), (offset, whence))?;
            Ok(res.extract::<u64>()?)
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
    pub fn tell(&mut self) -> io::Result<u64> {
        Python::attach(|py| self.bind(py).tell())
    }

    /// See [`BoundFile::seek_to`].
    pub fn seek_to(&mut self, cookie: u64) -> io::Result<u64> {
        Python::attach(|py| self.bind(py).seek_to(cookie))
    }

    /// See [`BoundFile::rewind`].
    pub fn rewind(&mut self) -> io::Result<u64> {
        Python::attach(|py| self.bind(py).rewind())
    }

    /// See [`BoundFile::seek_to_end`].
    pub fn seek_to_end(&mut self) -> io::Result<u64> {
        Python::attach(|py| self.bind(py).seek_to_end())
    }
}
