//! One interpretation of Python write progress for both byte and character streams.

use crate::Error;
use pyo3::exceptions::{PyAttributeError, PyBlockingIOError};
use pyo3::intern;
use pyo3::prelude::*;

pub(crate) fn write_result<'py>(
    py: Python<'py>,
    result: PyResult<Bound<'py, PyAny>>,
    available: usize,
    unit: &'static str,
) -> Result<usize, Error> {
    let written = match result {
        Ok(value) => {
            if value.is_none() {
                return Err(Error::would_block_write());
            }
            value.extract::<usize>()?
        }
        Err(err) if err.is_instance_of::<PyBlockingIOError>(py) => {
            // Buffered Python writers may accept some input before blocking. Returning Err
            // after that progress would tell Rust callers to retry already-consumed input.
            let count = match err.value(py).getattr(intern!(py, "characters_written")) {
                Ok(count) => count.extract::<usize>()?,
                Err(missing) if missing.is_instance_of::<PyAttributeError>(py) => {
                    return Err(err.into());
                }
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
        });
    }
    Ok(written)
}
