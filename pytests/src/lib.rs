//! An extension exercising every shape `pyo3-file-typed` offers, plus the untyped `pyo3-file`
//! equivalents so the two can be compared side by side from Python.

use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_file_typed::{
    BinaryFileno, BinaryRead, BinaryReadSeek, BinaryReadWrite, BinaryReadWriteSeekFileno,
    BinaryWrite, TextFileno, TextRead, TextReadSeek, TextReadWrite, TextWrite, Unchecked,
};
use std::io::{Read, Seek, SeekFrom, Write};

// ---------------------------------------------------------------- binary

/// Reads the whole stream. One `read(-1)` rather than a call per chunk.
#[pyfunction]
fn binary_read_all(py: Python<'_>, mut file: BinaryRead) -> PyResult<Py<PyBytes>> {
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(PyBytes::new(py, &buffer).unbind())
}

/// One `read(n)` into a fixed buffer: exactly the bytes Python handed over.
#[pyfunction]
fn binary_read_exactly(py: Python<'_>, mut file: BinaryRead, n: usize) -> PyResult<Py<PyBytes>> {
    let mut buffer = vec![0u8; n];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(PyBytes::new(py, &buffer).unbind())
}

#[pyfunction]
fn binary_write(mut file: BinaryWrite, data: &[u8]) -> PyResult<usize> {
    file.write_all(data)?;
    file.flush()?;
    Ok(data.len())
}

/// Reads, seeks back to the start and reads again, to show `Seek` working on bytes.
#[pyfunction]
fn binary_seek_roundtrip(py: Python<'_>, mut file: BinaryReadSeek) -> PyResult<Py<PyBytes>> {
    let mut first = vec![0u8; 4];
    file.read_exact(&mut first)?;
    file.seek(SeekFrom::Start(0))?;
    let mut all = Vec::new();
    file.read_to_end(&mut all)?;
    Ok(PyBytes::new(py, &all).unbind())
}

#[pyfunction]
fn binary_read_write(mut file: BinaryReadWrite, data: &[u8]) -> PyResult<usize> {
    file.write_all(data)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(buffer.len())
}

#[pyfunction]
fn binary_everything(mut file: BinaryReadWriteSeekFileno, data: &[u8]) -> PyResult<(usize, i32)> {
    file.write_all(data)?;
    file.seek(SeekFrom::Start(0))?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    let fd = file.fileno()?;
    Ok((buffer.len(), fd))
}

#[pyfunction]
fn binary_fileno(file: BinaryFileno) -> PyResult<i32> {
    Ok(file.fileno()?)
}

/// Raw descriptor compatibility; this does not promise a borrowed descriptor lifetime.
#[pyfunction]
fn binary_fileno_via_as_raw_fd(file: BinaryFileno) -> PyResult<i32> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        Ok(file.as_raw_fd())
    }
    #[cfg(not(unix))]
    {
        let _ = file;
        Ok(-1)
    }
}

// ---------------------------------------------------------------- text

/// Reads the whole stream as text. One `read(-1)` and one `str` across the boundary.
#[pyfunction]
fn text_read_all(mut file: TextRead) -> PyResult<String> {
    Ok(file.read_to_string()?)
}

/// Reads at most `n` **characters**, the unit Python counts a text read in.
#[pyfunction]
fn text_read_chars(mut file: TextRead, n: usize) -> PyResult<String> {
    Ok(file.read_chars(n)?)
}

#[pyfunction]
fn text_write(mut file: TextWrite, text: &str) -> PyResult<usize> {
    file.write_all_str(text)?;
    file.flush()?;
    Ok(text.chars().count())
}

#[pyfunction]
fn text_read_write(mut file: TextReadWrite, text: &str) -> PyResult<usize> {
    file.write_all_str(text)?;
    Ok(file.read_to_string()?.chars().count())
}

/// Uses `tell` to make a cookie and `seek_to` to come back to it.
#[pyfunction]
fn text_seek_roundtrip(mut file: TextReadSeek, n: usize) -> PyResult<(String, String)> {
    let cookie = file.tell()?;
    let first = file.read_chars(n)?;
    file.seek_to(cookie)?;
    let again = file.read_chars(n)?;
    Ok((first, again))
}

#[pyfunction]
fn text_fileno(file: TextFileno) -> PyResult<i32> {
    Ok(file.fileno()?)
}

// ---------------------------------------------------------------- pyo3-file, for comparison

/// The untyped equivalent of [`binary_read_all`], to compare against.
#[pyfunction]
fn legacy_read_all(py: Python<'_>, obj: Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let mut file = pyo3_file::PyFileLikeObject::py_new(obj)?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(PyBytes::new(py, &buffer).unbind())
}

/// The untyped equivalent of [`text_read_all`]: it goes through `std::io::Read`, so the
/// characters are encoded to UTF-8 by pyo3-file and decoded again here.
#[pyfunction]
fn legacy_read_all_text(obj: Bound<'_, PyAny>) -> PyResult<String> {
    let mut file = pyo3_file::PyFileLikeObject::py_new(obj)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

/// A single fixed-size `pyo3-file` read, so the result does not depend on `read_to_end`'s
/// buffer bookkeeping. Shows plainly what the Rust side receives.
#[pyfunction]
fn legacy_read_once(py: Python<'_>, obj: Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let mut file = pyo3_file::PyFileLikeObject::py_new(obj)?;
    let mut buffer = vec![0u8; 4096];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(PyBytes::new(py, &buffer).unbind())
}

/// Reads `n` characters the `pyo3-file` way, for a like-for-like comparison with
/// [`text_read_chars`]: a buffer four times the character count, one `read`, then a UTF-8
/// decode of what `pyo3-file` encoded on the way in.
#[pyfunction]
fn legacy_read_chars(obj: Bound<'_, PyAny>, n: usize) -> PyResult<String> {
    let mut file = pyo3_file::PyFileLikeObject::py_new(obj)?;
    let mut buffer = vec![0u8; n * 4];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    String::from_utf8(buffer).map_err(|err| PyErr::new::<pyo3::exceptions::PyValueError, _>(err.to_string()))
}

/// The untyped equivalent of [`binary_fileno`]. `pyo3-file` exposes this through `AsRawFd`,
/// which cannot report failure, so it panics when `fileno()` raises.
#[pyfunction]
fn legacy_fileno(obj: Bound<'_, PyAny>) -> PyResult<i32> {
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let file = pyo3_file::PyFileLikeObject::py_new(obj)?;
        Ok(file.as_raw_fd())
    }
    #[cfg(not(unix))]
    {
        let _ = obj;
        Ok(-1)
    }
}

/// The untyped equivalent of [`binary_write`]. Writing non-UTF-8 bytes to a text object panics,
/// because `pyo3-file` has to decode them to hand Python a `str`.
#[pyfunction]
fn legacy_write(obj: Bound<'_, PyAny>, data: &[u8]) -> PyResult<usize> {
    let mut file = pyo3_file::PyFileLikeObject::py_new(obj)?;
    file.write_all(data)?;
    Ok(data.len())
}

/// Moves the file onto a thread of its own, with this thread releasing the GIL entirely.
///
/// The child holds no token; each read attaches for as long as it needs and no longer. A
/// GIL-bound form could not leave this thread at all.
#[pyfunction]
fn read_on_another_thread(py: Python<'_>, file: BinaryRead) -> PyResult<usize> {
    let worker = std::thread::spawn(move || -> std::io::Result<usize> {
        let mut file = file;
        let mut sink = Vec::new();
        file.read_to_end(&mut sink)?;
        Ok(sink.len())
    });
    // Released here, so the child has to acquire it for itself.
    py.detach(|| worker.join())
        .map_err(|_| pyo3::exceptions::PyRuntimeError::new_err("the reader thread panicked"))?
        .map_err(Into::into)
}

/// The same read with the token held throughout: no attaching happens inside the loop.
#[pyfunction]
fn binary_read_all_bound(py: Python<'_>, file: BinaryRead) -> PyResult<Py<PyBytes>> {
    let mut file = file.into_bound(py);
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(PyBytes::new(py, &buffer).unbind())
}

/// Uses the escape hatch: no payload-kind check, capability checks kept.
#[pyfunction]
fn text_read_all_unchecked(mut file: Unchecked<TextRead>) -> PyResult<String> {
    Ok(file.read_to_string()?)
}

// ------------------------------------------------- pyo3-filelike, for comparison

/// `pyo3-filelike` splits binary and text into two types, so this is the closest equivalent of
/// [`binary_read_all`]. `PyBinaryFile::new` is private, so `From` is the only way in and its
/// `unwrap` turns a rejected file into a panic.
#[pyfunction]
fn filelike_read_all(py: Python<'_>, obj: Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let mut file = pyo3_filelike::PyBinaryFile::from(obj);
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer)?;
    Ok(PyBytes::new(py, &buffer).unbind())
}

/// A single fixed-size read through `PyBinaryFile`.
#[pyfunction]
fn filelike_read_once(py: Python<'_>, obj: Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let mut file = pyo3_filelike::PyBinaryFile::from(obj);
    let mut buffer = vec![0u8; 4096];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(PyBytes::new(py, &buffer).unbind())
}

/// `PyTextFile` implements `std::io::Read`, so a text object still reaches a bytes-oriented
/// consumer, re-encoded as UTF-8. This is what that consumer receives.
#[pyfunction]
fn filelike_text_as_bytes(py: Python<'_>, obj: Bound<'_, PyAny>) -> PyResult<Py<PyBytes>> {
    let mut file = pyo3_filelike::PyTextFile::from(obj);
    let mut buffer = vec![0u8; 4096];
    let read = file.read(&mut buffer)?;
    buffer.truncate(read);
    Ok(PyBytes::new(py, &buffer).unbind())
}

/// Whole-stream read through `PyTextFile`, for the benchmark.
#[pyfunction]
fn filelike_text_read_all(obj: Bound<'_, PyAny>) -> PyResult<String> {
    let mut file = pyo3_filelike::PyTextFile::from(obj);
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    Ok(text)
}

/// `pyo3-filelike` exposes the descriptor through `AsFd`, which cannot report failure.
#[pyfunction]
fn filelike_fileno(obj: Bound<'_, PyAny>) -> PyResult<i32> {
    #[cfg(unix)]
    {
        use std::os::fd::{AsFd, AsRawFd};
        let file = pyo3_filelike::PyBinaryFile::from(obj);
        Ok(file.as_fd().as_raw_fd())
    }
    #[cfg(not(unix))]
    {
        let _ = obj;
        Ok(-1)
    }
}

/// `PyBinaryFile` implements `Write` unconditionally, whatever the caller asked for.
#[pyfunction]
fn filelike_write(obj: Bound<'_, PyAny>, data: &[u8]) -> PyResult<usize> {
    let mut file = pyo3_filelike::PyBinaryFile::from(obj);
    file.write_all(data)?;
    Ok(data.len())
}

#[pymodule]
mod pyo3_file_typed_tests {
    #[pymodule_export]
    use super::{
        binary_everything, binary_fileno, binary_read_all, binary_read_exactly, binary_read_write,
        binary_seek_roundtrip, binary_write, legacy_fileno, legacy_read_all,
        legacy_read_all_text, legacy_read_chars, legacy_read_once, legacy_write, text_fileno, text_read_all, text_read_chars,
        binary_fileno_via_as_raw_fd, binary_read_all_bound, read_on_another_thread, filelike_fileno, filelike_read_all, filelike_read_once, filelike_text_as_bytes,
        filelike_text_read_all, filelike_write, text_read_all_unchecked, text_read_write, text_seek_roundtrip, text_write,
    };
}
