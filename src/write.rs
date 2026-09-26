//! Write and flush logic shared by byte and character streams.

use crate::Error;
use pyo3::exceptions::{PyAttributeError, PyBlockingIOError};
use pyo3::intern;
use pyo3::prelude::*;
use std::io;

/// Interprets what a Python `write` returned for `available` bytes or characters.
pub(crate) fn write_result<'py>(
    py: Python<'py>,
    result: PyResult<Bound<'py, PyAny>>,
    available: usize,
    unit: &'static str,
) -> io::Result<usize> {
    let written = match result {
        Ok(value) if value.is_none() => return Err(Error::would_block_write().into()),
        Ok(value) => value.extract::<usize>()?,
        Err(err) if err.is_instance_of::<PyBlockingIOError>(py) => {
            // A buffered writer may accept some input before blocking; reporting an error then
            // would make the caller resend input that was already consumed.
            let count = match err.value(py).getattr(intern!(py, "characters_written")) {
                Ok(count) => count.extract::<usize>()?,
                Err(missing) if missing.is_instance_of::<PyAttributeError>(py) => 0,
                Err(other) => return Err(other.into()),
            };
            if count == 0 {
                return Err(err.into());
            }
            count
        }
        Err(err) => return Err(err.into()),
    };
    if written > available {
        return Err(Error::ImpossibleWriteCount {
            reported: written,
            available,
            unit,
        }
        .into());
    }
    Ok(written)
}

/// Calls `flush` if the object has one; one without has nothing to flush.
pub(crate) fn flush(obj: &Bound<'_, PyAny>) -> io::Result<()> {
    let flush = intern!(obj.py(), "flush");
    if obj.hasattr(flush)? {
        obj.call_method0(flush)?;
    }
    Ok(())
}
