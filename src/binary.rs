//! [`std::io`] implementations for binary files.
//!
//! Every byte here is a byte the Python object produced or consumed. `read(n)` asks Python for `n`
//! bytes and `write` hands it `bytes`, so the counts mean the same thing on both sides and nothing
//! is decoded on the way through.
//!
//! The work lives on [`BoundPyBinaryFile`], which already has a token. [`PyBinaryFile`] implements
//! the same traits by attaching once and delegating, so a caller who has the GIL can avoid that
//! and a caller who does not — a thread of its own, say — still works.

use crate::{Binary, BoundPyBinaryFile, Error, PyBinaryFile};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyMemoryView};
use std::io::{self, Read, Seek, SeekFrom, Write};

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool> Read
    for BoundPyBinaryFile<'_, true, WRITE, SEEK, FILENO>
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let obj = self.as_py_object();
        let py = obj.py();
        (|| -> Result<usize, Error> {
            let res = obj.call_method1(intern!(py, "read"), (buf.len(),))?;
            if res.is_none() {
                return Err(Error::would_block_read());
            }
            // Bytes can be borrowed directly. Other buffers need a snapshot: interpreting
            // them as a sequence loses multibyte formats and accepts non-buffer lists.
            // memoryview.tobytes() safely flattens even strided buffers in C order, without
            // exposing a borrowed slice of potentially mutable Python memory to Rust.
            let snapshot;
            let bytes = if let Ok(bytes) = res.cast::<PyBytes>() {
                bytes.as_bytes()
            } else {
                snapshot = PyMemoryView::from(&res)
                    .and_then(|view| view.call_method0(intern!(py, "tobytes")))
                    .and_then(|bytes| bytes.cast_into::<PyBytes>().map_err(Into::into))
                    .map_err(Error::wrong_payload::<Binary>)?;
                snapshot.as_bytes()
            };
            if bytes.len() > buf.len() {
                return Err(Error::OverlongRead {
                    got: bytes.len(),
                    asked: buf.len(),
                });
            }
            buf[..bytes.len()].copy_from_slice(bytes);
            Ok(bytes.len())
        })()
        .map_err(Into::into)
    }

    // Keep std's bulk defaults: they use bounded reads, retry Interrupted, and preserve
    // read_to_string's partial-I/O-error and invalid-UTF-8 behavior.
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool> Write
    for BoundPyBinaryFile<'_, READ, true, SEEK, FILENO>
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let obj = self.as_py_object();
        let py = obj.py();
        crate::write::write_result(
            py,
            obj.call_method1(intern!(py, "write"), (PyBytes::new(py, buf),)),
            buf.len(),
            "bytes",
        )
        .map_err(Into::into)
    }

    /// Flushes the object, or does nothing if it has no `flush`.
    ///
    /// An object with no `flush` has nothing to flush, so requiring one would turn away every
    /// minimal writer that implements nothing else.
    fn flush(&mut self) -> io::Result<()> {
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

impl<const READ: bool, const WRITE: bool, const FILENO: bool> Seek
    for BoundPyBinaryFile<'_, READ, WRITE, true, FILENO>
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

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool> Read
    for PyBinaryFile<true, WRITE, SEEK, FILENO>
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Python::attach(|py| self.bind(py).read(buf))
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> io::Result<()> {
        Python::attach(|py| self.bind(py).read_exact(buf))
    }

    fn read_to_end(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        Python::attach(|py| self.bind(py).read_to_end(out))
    }

    fn read_to_string(&mut self, out: &mut String) -> io::Result<usize> {
        Python::attach(|py| self.bind(py).read_to_string(out))
    }
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool> Write
    for PyBinaryFile<READ, true, SEEK, FILENO>
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Python::attach(|py| self.bind(py).write(buf))
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        // One attach for the whole retry loop rather than one per short write.
        Python::attach(|py| self.bind(py).write_all(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        Python::attach(|py| self.bind(py).flush())
    }
}

impl<const READ: bool, const WRITE: bool, const FILENO: bool> Seek
    for PyBinaryFile<READ, WRITE, true, FILENO>
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Python::attach(|py| self.bind(py).seek(pos))
    }
}
