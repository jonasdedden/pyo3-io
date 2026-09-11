//! Benchmark entry points: setup/reset belongs to Python, adapter lifetime to Rust.
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use pyo3_typed_io::{BinaryRead, BinaryWrite, TextRead};
use std::io::{Read, Write};

fn unknown(implementation: &str) -> PyErr {
    PyValueError::new_err(format!("unknown implementation: {implementation}"))
}

/// Measures only construction and destruction, without performing an I/O operation.
#[pyfunction]
pub fn bench_construct(obj: Bound<'_, PyAny>, implementation: &str, count: usize) -> PyResult<()> {
    if count == 0 {
        return Err(PyValueError::new_err("count must be positive"));
    }
    for _ in 0..count {
        match implementation {
            "typed-detached" => {
                std::hint::black_box(obj.extract::<BinaryRead>()?);
            }
            "typed-bound" => {
                std::hint::black_box(obj.extract::<BinaryRead>()?.into_bound(obj.py()));
            }
            "legacy" => {
                std::hint::black_box(pyo3_file::PyFileLikeObject::py_new(obj.clone())?);
            }
            "legacy-checked" => {
                std::hint::black_box(pyo3_file::PyFileLikeObject::py_with_requirements(
                    obj.clone(),
                    true,
                    false,
                    false,
                    false,
                )?);
            }
            "filelike" => {
                std::hint::black_box(pyo3_filelike::PyBinaryFile::from(obj.clone()));
            }
            _ => return Err(unknown(implementation)),
        }
    }
    Ok(())
}

fn read<R: Read>(mut file: R, chunk: usize, verify: bool) -> std::io::Result<(usize, Vec<u8>)> {
    let mut result = Vec::new();
    if chunk == 0 {
        file.read_to_end(&mut result)?;
        let count = result.len();
        if !verify {
            result.clear();
        }
        return Ok((count, result));
    }
    let mut buffer = vec![0; chunk];
    let mut count = 0;
    loop {
        let n = match file.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            other => other?,
        };
        if n == 0 {
            break;
        }
        count += n;
        if verify {
            result.extend_from_slice(&buffer[..n]);
        }
    }
    Ok((count, result))
}

/// chunk=0 uses read_to_end; positive chunks reuse one fixed buffer.
/// verify=true copies the complete result, for untimed correctness checks only.
#[pyfunction]
pub fn bench_read(
    obj: Bound<'_, PyAny>,
    implementation: &str,
    chunk: usize,
    verify: bool,
) -> PyResult<(usize, Option<Py<PyBytes>>)> {
    if chunk > isize::MAX as usize {
        return Err(PyValueError::new_err("chunk exceeds maximum buffer size"));
    }
    let (count, data) = match implementation {
        "typed-detached" => read(obj.extract::<BinaryRead>()?, chunk, verify)?,
        "typed-bound" => read(
            obj.extract::<BinaryRead>()?.into_bound(obj.py()),
            chunk,
            verify,
        )?,
        "legacy" => read(
            pyo3_file::PyFileLikeObject::py_new(obj.clone())?,
            chunk,
            verify,
        )?,
        "filelike" => read(
            pyo3_filelike::PyBinaryFile::from(obj.clone()),
            chunk,
            verify,
        )?,
        _ => return Err(unknown(implementation)),
    };
    Ok((
        count,
        verify.then(|| PyBytes::new(obj.py(), &data).unbind()),
    ))
}

fn write<W: Write>(mut file: W, data: &[u8], count: usize, flush: bool) -> std::io::Result<usize> {
    for _ in 0..count {
        file.write_all(data)?;
    }
    if flush {
        file.flush()?;
    }
    Ok(data.len() * count)
}

/// Constructs once and reuses the borrowed input slice; flush is identical for all paths.
#[pyfunction]
pub fn bench_write(
    obj: Bound<'_, PyAny>,
    implementation: &str,
    data: &[u8],
    count: usize,
    flush: bool,
) -> PyResult<usize> {
    if count == 0 || data.is_empty() || data.len().checked_mul(count).is_none() {
        return Err(PyValueError::new_err(
            "nonempty data and positive, nonoverflowing count required",
        ));
    }
    Ok(match implementation {
        "typed-detached" => write(obj.extract::<BinaryWrite>()?, data, count, flush)?,
        "typed-bound" => write(
            obj.extract::<BinaryWrite>()?.into_bound(obj.py()),
            data,
            count,
            flush,
        )?,
        "legacy" => write(
            pyo3_file::PyFileLikeObject::py_new(obj)?,
            data,
            count,
            flush,
        )?,
        "filelike" => write(pyo3_filelike::PyBinaryFile::from(obj), data, count, flush)?,
        _ => return Err(unknown(implementation)),
    })
}

/// Whole text streams only: the unit of work is the identical Unicode string.
/// Legacy/filelike UTF-8 conversion is part of their adaptation cost.
#[pyfunction]
pub fn bench_text_read(obj: Bound<'_, PyAny>, implementation: &str) -> PyResult<String> {
    Ok(match implementation {
        "typed-detached" => obj.extract::<TextRead>()?.read_to_string()?,
        "typed-bound" => obj
            .extract::<TextRead>()?
            .into_bound(obj.py())
            .read_to_string()?,
        "legacy" => {
            let mut result = String::new();
            pyo3_file::PyFileLikeObject::py_new(obj)?.read_to_string(&mut result)?;
            result
        }
        "filelike" => {
            let mut result = String::new();
            pyo3_filelike::PyTextFile::from(obj).read_to_string(&mut result)?;
            result
        }
        _ => return Err(unknown(implementation)),
    })
}
