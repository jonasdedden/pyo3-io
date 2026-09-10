//! [`std::io`] implementations for [`PyFile<Binary, ..>`].
//!
//! Every byte here is a byte the Python object produced or consumed. `read(n)` asks Python for `n`
//! bytes and `write` hands it `bytes`, so the counts mean the same thing on both sides and nothing
//! is decoded on the way through.

use crate::{Binary, Error, PyFile};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::borrow::Cow;
use std::io::{self, Read, Seek, SeekFrom, Write};

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool> Read
    for PyFile<Binary, true, WRITE, SEEK, FILENO>
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        Python::attach(|py| -> Result<usize, Error> {
            let obj = self.as_py_object().bind(py);
            let res = obj.call_method1(intern!(py, "read"), (buf.len(),))?;
            if res.is_none() {
                return Err(Error::would_block_read());
            }
            let bytes = res
                .extract::<Cow<'_, [u8]>>()
                .map_err(Error::wrong_payload::<Binary>)?;
            if bytes.len() > buf.len() {
                return Err(Error::OverlongRead {
                    got: bytes.len(),
                    asked: buf.len(),
                });
            }
            buf[..bytes.len()].copy_from_slice(&bytes);
            Ok(bytes.len())
        })
        .map_err(Into::into)
    }

    /// Overridden to ask Python for the whole stream at once.
    ///
    /// The default implementation would cross the language boundary once per buffer-sized chunk,
    /// growing the buffer as it goes. `read(-1)` gets the same bytes in a single call.
    fn read_to_end(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        Python::attach(|py| -> Result<usize, Error> {
            let obj = self.as_py_object().bind(py);
            let before = out.len();
            // Usually one call plus an empty one; the loop is only there for objects that hand
            // back a partial result, which `read(-1)` is permitted to do.
            loop {
                let res = obj.call_method1(intern!(py, "read"), (-1i64,))?;
                if res.is_none() {
                    return Err(Error::would_block_read());
                }
                // Borrowed from the Python object and copied straight into `out`: extracting an
                // owned `Vec` first would copy the whole stream an extra time.
                let chunk = res
                    .extract::<Cow<'_, [u8]>>()
                    .map_err(Error::wrong_payload::<Binary>)?;
                if chunk.is_empty() {
                    break;
                }
                out.extend_from_slice(&chunk);
            }
            Ok(out.len() - before)
        })
        .map_err(Into::into)
    }

    /// Overridden for the same reason as [`read_to_end`](Read::read_to_end).
    ///
    /// The bytes still have to be UTF-8 checked, because a binary object promises nothing about
    /// its encoding. If the object is a text stream on the Python side, ask for it as a
    /// [`PyFile<Text, ..>`](crate::Text) instead and skip the check entirely.
    fn read_to_string(&mut self, out: &mut String) -> io::Result<usize> {
        let mut bytes = Vec::new();
        let read = self.read_to_end(&mut bytes)?;
        let text = String::from_utf8(bytes)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
        if out.is_empty() {
            *out = text;
        } else {
            out.push_str(&text);
        }
        Ok(read)
    }
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool> Write
    for PyFile<Binary, READ, true, SEEK, FILENO>
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Python::attach(|py| -> Result<usize, Error> {
            let obj = self.as_py_object().bind(py);
            let res = obj.call_method1(intern!(py, "write"), (PyBytes::new(py, buf),))?;
            if res.is_none() {
                return Err(Error::would_block_write());
            }
            let written = res.extract::<usize>()?;
            if written > buf.len() {
                return Err(Error::ImpossibleWriteCount {
                    reported: written,
                    available: buf.len(),
                    unit: "bytes",
                });
            }
            Ok(written)
        })
        .map_err(Into::into)
    }

    /// Flushes the object, or does nothing if it has no `flush`.
    ///
    /// An object with no `flush` has nothing to flush, so requiring one would turn away every
    /// minimal writer that implements nothing else.
    fn flush(&mut self) -> io::Result<()> {
        Python::attach(|py| -> Result<(), Error> {
            let obj = self.as_py_object().bind(py);
            let flush = intern!(py, "flush");
            if obj.hasattr(flush)? {
                obj.call_method0(flush)?;
            }
            Ok(())
        })
        .map_err(Into::into)
    }
}

impl<const READ: bool, const WRITE: bool, const FILENO: bool> Seek
    for PyFile<Binary, READ, WRITE, true, FILENO>
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (offset, whence) = match pos {
            SeekFrom::Start(offset) => (
                i64::try_from(offset).map_err(|_| Error::OutOfRange {
                    what: "seek offset",
                })?,
                0,
            ),
            SeekFrom::Current(offset) => (offset, 1),
            SeekFrom::End(offset) => (offset, 2),
        };
        Python::attach(|py| -> Result<u64, Error> {
            let obj = self.as_py_object().bind(py);
            let res = obj.call_method1(intern!(py, "seek"), (offset, whence))?;
            Ok(res.extract::<u64>()?)
        })
        .map_err(Into::into)
    }
}
