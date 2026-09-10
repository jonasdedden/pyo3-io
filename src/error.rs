//! What can go wrong, and how it reaches Rust and Python.

use crate::Mode;
use pyo3::exceptions::{PyBlockingIOError, PyTypeError};
use pyo3::PyErr;
use std::io;

/// Everything this crate can fail with.
///
/// Both destinations are covered: `?` in a function returning [`std::io::Result`] converts through
/// [`From<Error> for io::Error`](#impl-From<Error>-for-Error), preserving a meaningful
/// [`io::ErrorKind`], and `?` in a `#[pyfunction]` converts to the Python exception that matches.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The object returned `None`, which Python streams use for "nothing right now".
    ///
    /// This is not fatal. A non-blocking stream returns `None` when no data has arrived yet and
    /// real data on a later call, so this maps to [`io::ErrorKind::WouldBlock`] and reaches Python
    /// as `BlockingIOError` — the same thing a buffered stream would have raised by itself.
    #[error(
        "the object returned None from {operation}(), meaning nothing could be {done} right now. \
         This is what a non-blocking stream does when it is not ready; a later call may succeed. \
         (If the object meant end of stream, it should return an empty bytes or str instead.)"
    )]
    WouldBlock {
        /// `read` or `write`.
        operation: &'static str,
        /// `read` or `written`.
        done: &'static str,
    },

    /// The object deals in the other payload after all.
    ///
    /// Only reachable for an object that identified itself as neither kind, which is what keeps
    /// duck typing working; see the crate documentation.
    #[error(
        "read() did not return {expected} ({source}). This is a {kind} file-like object, and the \
         object did not identify itself as {other}, so it was taken at its word. If it deals in \
         {other_payload}, ask for it as a {other} PyFile, or wrap it with io.TextIOWrapper on the \
         Python side."
    )]
    WrongPayload {
        /// What the read should have produced, e.g. `bytes`.
        expected: &'static str,
        /// The kind that was asked for, e.g. `binary`.
        kind: &'static str,
        /// The other kind, e.g. `text`.
        other: &'static str,
        /// The other kind's payload, e.g. `str`.
        other_payload: &'static str,
        /// The extraction failure underneath.
        source: PyErr,
    },

    /// `read` ignored the size it was given and handed back more.
    #[error("read() returned {got} bytes after being asked for at most {asked}")]
    OverlongRead {
        /// What arrived.
        got: usize,
        /// What was asked for.
        asked: usize,
    },

    /// `write` claimed to have consumed more than it was given.
    #[error("write() reported {reported} {unit} written of {available}")]
    ImpossibleWriteCount {
        /// What the object claimed.
        reported: usize,
        /// What it was actually given.
        available: usize,
        /// `bytes` or `characters`.
        unit: &'static str,
    },

    /// `write` kept accepting nothing, so writing everything is not going to happen.
    #[error("the object accepted no {unit}")]
    WroteNothing {
        /// `bytes` or `characters`.
        unit: &'static str,
    },

    /// A count or position that does not fit in the `int` Python is going to be handed.
    #[error("{what} out of range")]
    OutOfRange {
        /// What was out of range.
        what: &'static str,
    },

    /// The object is missing a method the requested capabilities need.
    #[error("object of type {type_name} has no .{method}() method")]
    MissingMethod {
        /// The Python type.
        type_name: String,
        /// The method that is missing.
        method: &'static str,
    },

    /// The object identified itself as the other payload kind.
    #[error(
        "expected a {wanted} file-like object, got the {got} object {type_name} ({why}). {hint}"
    )]
    WrongKind {
        /// The kind that was asked for.
        wanted: &'static str,
        /// The kind the object turned out to be.
        got: &'static str,
        /// The Python type.
        type_name: String,
        /// Which rung of the classification fired.
        why: &'static str,
        /// What to do about it.
        hint: &'static str,
    },

    /// Anything raised on the Python side.
    #[error(transparent)]
    Python(#[from] PyErr),
}

impl Error {
    pub(crate) fn would_block_read() -> Self {
        Self::WouldBlock {
            operation: "read",
            done: "read",
        }
    }

    pub(crate) fn would_block_write() -> Self {
        Self::WouldBlock {
            operation: "write",
            done: "written",
        }
    }

    /// The mismatch for a `M`-kind file whose object turned out to be the other kind.
    pub(crate) fn wrong_payload<M: Mode>(source: PyErr) -> Self {
        Self::WrongPayload {
            expected: M::PAYLOAD,
            kind: M::NAME,
            other: M::OTHER_NAME,
            other_payload: M::OTHER_PAYLOAD,
            source,
        }
    }
}

/// Keeps a meaningful [`io::ErrorKind`] rather than flattening everything to `Other`.
impl From<Error> for io::Error {
    fn from(err: Error) -> Self {
        let kind = match &err {
            Error::WouldBlock { .. } => io::ErrorKind::WouldBlock,
            Error::WrongPayload { .. }
            | Error::OverlongRead { .. }
            | Error::ImpossibleWriteCount { .. } => io::ErrorKind::InvalidData,
            Error::WroteNothing { .. } => io::ErrorKind::WriteZero,
            Error::OutOfRange { .. } => io::ErrorKind::InvalidInput,
            Error::MissingMethod { .. } | Error::WrongKind { .. } => io::ErrorKind::InvalidInput,
            // `pyo3` already maps the OS-shaped exceptions, including BlockingIOError, so a
            // stream that raises rather than returning None lands on the same kind.
            Error::Python(_) => {
                let Error::Python(err) = err else {
                    unreachable!()
                };
                return err.into();
            }
        };
        io::Error::new(kind, err)
    }
}

impl From<Error> for PyErr {
    fn from(err: Error) -> Self {
        match err {
            Error::Python(err) => err,
            // These are the extraction-time checks, and a wrong argument is a TypeError.
            err @ (Error::MissingMethod { .. } | Error::WrongKind { .. }) => {
                PyTypeError::new_err(err.to_string())
            }
            err @ Error::WouldBlock { .. } => PyBlockingIOError::new_err(err.to_string()),
            err => io::Error::from(err).into(),
        }
    }
}
