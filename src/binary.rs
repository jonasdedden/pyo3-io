//! [`std::io`] implementations for binary files.
//!
//! Every byte here is a byte the Python object produced or consumed. `read(n)` asks Python for `n`
//! bytes and `write` hands it `bytes`, so the counts mean the same thing on both sides and nothing
//! is decoded on the way through.
//!
//! The work lives on [`BoundPyBinaryIO`], which already has a token. [`PyBinaryIO`] implements
//! the same traits by attaching once and delegating, so a caller who has the GIL can avoid that
//! and a caller who does not — a thread of its own, say — still works.

use crate::{BinaryPayload, BoundPyBinaryIO, Error, PyBinaryIO};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyMemoryView};
use std::io::{self, Read, Seek, SeekFrom, Write};

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool>
    BoundPyBinaryIO<'_, true, WRITE, SEEK, FILENO>
{
    /// One `read(size)` call, handing the bytes to `consume` without copying them first.
    ///
    /// `consume` sees exactly the bytes Python returned, at most `size` of them, and is not called
    /// at all on error.
    fn read_with(&self, size: usize, consume: impl FnOnce(&[u8])) -> Result<usize, Error> {
        let obj = self.as_py_object();
        let py = obj.py();
        let res = obj.call_method1(intern!(py, "read"), (size,))?;
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
                .map_err(Error::wrong_payload::<BinaryPayload>)?;
            snapshot.as_bytes()
        };
        if bytes.len() > size {
            return Err(Error::OverlongRead {
                got: bytes.len(),
                asked: size,
            });
        }
        consume(bytes);
        Ok(bytes.len())
    }
}

/// The first read size of [`read_to_end`](Read::read_to_end), as std's default buffer.
const FIRST_BULK_READ: usize = 8 * 1024;
/// Bulk reads grow no further than this.
///
/// Each read makes Python allocate a fresh `bytes` of the requested size. Below glibc's default
/// 128 KiB mmap threshold those allocations are recycled from the heap and stay cache-warm;
/// letting them grow with the stream, as std's doubling does, made every call map and fault in
/// new pages, costing 2-4x on a 16 MiB `read_to_end` measured against `BytesIO` and real files,
/// buffered or not. Past 64 KiB the per-call overhead is already negligible.
const MAX_BULK_READ: usize = 64 * 1024;

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool> Read
    for BoundPyBinaryIO<'_, true, WRITE, SEEK, FILENO>
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.read_with(buf.len(), |bytes| buf[..bytes.len()].copy_from_slice(bytes))
            .map_err(Into::into)
    }

    /// Appends straight from each `bytes` object Python returns.
    ///
    /// Same contract as std's default: bounded positive-size reads until an empty one,
    /// `Interrupted` retried, and on error everything read so far stays in `out`. The default
    /// can only read into an initialised `&mut [u8]`, so it zero-fills the vector's spare
    /// capacity before every read, including the final one that finds end of stream.
    fn read_to_end(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        let start = out.len();
        // Start from capacity the caller reserved, as std does, within the bounds above.
        let mut size = (out.capacity() - start).clamp(FIRST_BULK_READ, MAX_BULK_READ);
        loop {
            match self.read_with(size, |bytes| out.extend_from_slice(bytes)) {
                Ok(0) => return Ok(out.len() - start),
                // Grow while the object keeps filling the request, as std does.
                Ok(got) if got == size => size = (size * 2).min(MAX_BULK_READ),
                Ok(_) => {}
                Err(err) => {
                    let err = io::Error::from(err);
                    if err.kind() != io::ErrorKind::Interrupted {
                        return Err(err);
                    }
                }
            }
        }
    }

    /// [`read_to_end`](Read::read_to_end), then std's UTF-8 rules.
    ///
    /// Nothing is appended unless everything read is valid UTF-8, and an I/O error takes
    /// precedence over invalid UTF-8, exactly as std's default. Only the new bytes are checked.
    fn read_to_string(&mut self, out: &mut String) -> io::Result<usize> {
        // An empty `out` lends its buffer, so reserved capacity is still used; otherwise the new
        // bytes are read apart from it, because they must be validated before being appended.
        let mut bytes = if out.is_empty() {
            std::mem::take(out).into_bytes()
        } else {
            Vec::new()
        };
        let result = self.read_to_end(&mut bytes);
        match String::from_utf8(bytes) {
            Ok(text) => {
                if out.is_empty() {
                    *out = text;
                } else {
                    out.push_str(&text);
                }
                result
            }
            Err(invalid) => result.and_then(|_| {
                Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    invalid.utf8_error(),
                ))
            }),
        }
    }
}

impl<const READ: bool, const SEEK: bool, const FILENO: bool> Write
    for BoundPyBinaryIO<'_, READ, true, SEEK, FILENO>
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
    for BoundPyBinaryIO<'_, READ, WRITE, true, FILENO>
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
    for PyBinaryIO<true, WRITE, SEEK, FILENO>
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
    for PyBinaryIO<READ, true, SEEK, FILENO>
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
    for PyBinaryIO<READ, WRITE, true, FILENO>
{
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        Python::attach(|py| self.bind(py).seek(pos))
    }
}
