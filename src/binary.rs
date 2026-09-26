//! [`std::io`] implementations for binary files, which pass bytes through undecoded.

use crate::{BinaryPayload, BoundPyBinaryIO, Error, PyBinaryIO};
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyMemoryView};
use std::io::{self, Read, Seek, SeekFrom, Write};

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool>
    BoundPyBinaryIO<'_, true, WRITE, SEEK, FILENO>
{
    /// One `read(size)` call, handing at most `size` bytes to `consume` without copying them
    /// first. `consume` is not called on error.
    fn read_with(&self, size: usize, consume: impl FnOnce(&[u8])) -> io::Result<usize> {
        let obj = self.as_py_object();
        let py = obj.py();
        let res = obj.call_method1(intern!(py, "read"), (size,))?;
        if res.is_none() {
            return Err(Error::would_block_read().into());
        }
        // Other buffers may be mutable or strided, so they are copied out with
        // memoryview.tobytes() rather than borrowed.
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
            }
            .into());
        }
        consume(bytes);
        Ok(bytes.len())
    }
}

/// The first read size of [`read_to_end`](Read::read_to_end), as std's default buffer.
const FIRST_BULK_READ: usize = 8 * 1024;
/// Bulk reads grow no further than this, keeping each `bytes` Python allocates below glibc's
/// 128 KiB mmap threshold. Growing unbounded made a 16 MiB `read_to_end` 2-4x slower.
const MAX_BULK_READ: usize = 64 * 1024;

impl<const WRITE: bool, const SEEK: bool, const FILENO: bool> Read
    for BoundPyBinaryIO<'_, true, WRITE, SEEK, FILENO>
{
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.read_with(buf.len(), |bytes| buf[..bytes.len()].copy_from_slice(bytes))
    }

    /// Std's contract, but appending each `bytes` Python returns rather than zero-filling spare
    /// capacity to read into.
    fn read_to_end(&mut self, out: &mut Vec<u8>) -> io::Result<usize> {
        let start = out.len();
        // Start from the capacity the caller reserved and double while reads fill up, as std does.
        let mut size = (out.capacity() - start).clamp(FIRST_BULK_READ, MAX_BULK_READ);
        loop {
            match self.read_with(size, |bytes| out.extend_from_slice(bytes)) {
                Ok(0) => return Ok(out.len() - start),
                Ok(got) if got == size => size = (size * 2).min(MAX_BULK_READ),
                Ok(_) => {}
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
                Err(err) => return Err(err),
            }
        }
    }

    /// Std's contract: nothing is appended unless all of it is UTF-8, and I/O errors win.
    fn read_to_string(&mut self, out: &mut String) -> io::Result<usize> {
        // Only an empty `out` can lend its buffer, as the new bytes are validated before appending.
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
    }

    /// Flushes the object, or does nothing if it has no `flush`.
    fn flush(&mut self) -> io::Result<()> {
        crate::write::flush(self.as_py_object())
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
        Ok(obj
            .call_method1(intern!(py, "seek"), (offset, whence))?
            .extract()?)
    }
}

// ---------------------------------------------------------------- detached forms
//
// Attach once and delegate. The looping provided methods are overridden too, so the whole loop
// runs under one attach.

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
