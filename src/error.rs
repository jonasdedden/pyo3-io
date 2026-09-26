//! The crate's error type and its conversions to `io::Error` and `PyErr`.

use crate::Payload;
use pyo3::exceptions::{PyBlockingIOError, PyTypeError};
use pyo3::PyErr;
use std::io;

/// Everything this crate can fail with.
///
/// Converts to an [`io::Error`] with a matching [`io::ErrorKind`], and to a matching Python
/// exception.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// The object returned `None`, which non-blocking Python streams use for "not ready yet".
    /// Maps to [`io::ErrorKind::WouldBlock`] and `BlockingIOError`.
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

    /// A read result was not the payload kind asked for, or was a `str` with lone surrogates.
    #[error(
        "read() did not return {expected} usable by a {kind} stream ({source}). \
         If it returns {other_payload}, use a {other} wrapper instead. Binary results must \
         expose a readable buffer; text must be representable as UTF-8."
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

    /// A text `read` returned more Unicode characters than requested.
    #[error("read() returned {got} characters after being asked for at most {asked}")]
    OverlongTextRead {
        /// The number of Unicode characters returned.
        got: usize,
        /// The requested maximum number of characters.
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

    /// A count or byte offset outside this Rust API's supported range.
    #[error("{what} out of range")]
    OutOfRange {
        /// What was out of range.
        what: &'static str,
    },

    /// An `io.IOBase` object's `readable()`, `writable()` or `seekable()` returned `False`.
    #[error(
        "object of type {type_name} reports {query}() is False for .{method}(). \
         It was extracted as a file with the {capability} capability."
    )]
    RefusesCapability {
        /// The Python type.
        type_name: String,
        /// `readable`, `writable` or `seekable`.
        query: &'static str,
        /// The method that would have been called.
        method: &'static str,
        /// How the capability is spelled in `PyIO`'s parameters.
        capability: &'static str,
    },

    /// A required operation is absent or not callable.
    #[error("object of type {type_name} has no .{method}() method (missing or not callable)")]
    MissingMethod {
        /// The Python type.
        type_name: String,
        /// The required operation.
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
        /// Which `io` base class it derives from.
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

    /// The mismatch for a `P`-kind file whose object turned out to be the other kind.
    pub(crate) fn wrong_payload<P: Payload>(source: PyErr) -> Self {
        Self::WrongPayload {
            expected: P::PAYLOAD,
            kind: P::NAME,
            other: P::OTHER_NAME,
            other_payload: P::OTHER_PAYLOAD,
            source,
        }
    }
}

impl From<Error> for io::Error {
    fn from(err: Error) -> Self {
        let kind = match err {
            // pyo3 maps OSError subclasses, including BlockingIOError, to their kinds.
            Error::Python(err) => return err.into(),
            Error::WouldBlock { .. } => io::ErrorKind::WouldBlock,
            Error::WrongPayload { .. }
            | Error::OverlongRead { .. }
            | Error::OverlongTextRead { .. }
            | Error::ImpossibleWriteCount { .. } => io::ErrorKind::InvalidData,
            Error::WroteNothing { .. } => io::ErrorKind::WriteZero,
            Error::OutOfRange { .. }
            | Error::MissingMethod { .. }
            | Error::RefusesCapability { .. }
            | Error::WrongKind { .. } => io::ErrorKind::InvalidInput,
        };
        io::Error::new(kind, err)
    }
}

impl From<Error> for PyErr {
    fn from(err: Error) -> Self {
        match err {
            Error::Python(err) => err,
            // Extraction-time checks: a wrong argument is a TypeError.
            err @ (Error::MissingMethod { .. }
            | Error::RefusesCapability { .. }
            | Error::WrongKind { .. }) => PyTypeError::new_err(err.to_string()),
            err @ Error::WouldBlock { .. } => PyBlockingIOError::new_err(err.to_string()),
            err => io::Error::from(err).into(),
        }
    }
}
