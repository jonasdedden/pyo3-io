#![doc = include_str!("../README.md")]

use pyo3::exceptions::{PyAttributeError, PyValueError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyString, PyType};
use pyo3::{intern, Borrowed, FromPyObject};
use std::marker::PhantomData;

mod binary;
mod error;
#[cfg(unix)]
mod fd;
#[cfg(feature = "experimental-inspect")]
mod introspection;
mod io_types;
mod payload;
mod text;
mod write;

pub use error::Error;
pub use payload::{BinaryPayload, Payload, TextPayload};
pub use text::PyTextPosition;

mod sealed {
    pub trait Sealed {}
}

/// A Python file-like object dealing in `bytes`, whose capabilities are part of its type.
///
/// `READ`, `WRITE` and `SEEK` enable [`std::io::Read`], [`std::io::Write`] and
/// [`std::io::Seek`]; shared operations are on [`PyIO`]. Prefer a [named alias](aliases) such as
/// [`PyBinaryRead`] over spelling out the parameters.
pub type PyBinaryIO<const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> =
    PyIO<BinaryPayload, READ, WRITE, SEEK, FILENO>;

/// A Python file-like object dealing in `str`, whose capabilities are part of its type.
///
/// Python counts text in characters, so instead of `std::io` this has a character API and seeks
/// by the opaque cookies `tell()` returns. Shared operations are on [`PyIO`]; prefer a
/// [named alias](aliases) such as [`PyTextRead`] over spelling out the parameters.
pub type PyTextIO<const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> =
    PyIO<TextPayload, READ, WRITE, SEEK, FILENO>;

/// [`PyBinaryIO`] tied to an attached Python token, from [`PyIO::bind`] or [`PyIO::into_bound`].
pub type BoundPyBinaryIO<
    'py,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> = BoundPyIO<'py, BinaryPayload, READ, WRITE, SEEK, FILENO>;

/// [`PyTextIO`] tied to an attached Python token, from [`PyIO::bind`] or [`PyIO::into_bound`].
pub type BoundPyTextIO<
    'py,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> = BoundPyIO<'py, TextPayload, READ, WRITE, SEEK, FILENO>;

pub mod aliases;

// Every alias is importable from the crate root, but only the common ones are listed there.
#[doc(hidden)]
pub use aliases::bound::*;
#[doc(hidden)]
pub use aliases::*;
#[doc(inline)]
pub use aliases::{
    PyBinaryRead, PyBinaryReadSeek, PyBinaryWrite, PyTextRead, PyTextReadSeek, PyTextWrite,
};

/// Construction, binding and object access shared by [`PyBinaryIO`] and [`PyTextIO`].
///
/// Use a [named alias](aliases) rather than spelling out the parameters.
pub struct PyIO<P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> {
    obj: Py<PyAny>,
    // `fn() -> P` rather than `P` so the auto traits and variance come from `Py<PyAny>` alone
    payload: PhantomData<fn() -> P>,
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> std::fmt::Debug
    for PyIO<P, READ, WRITE, SEEK, FILENO>
where
    P: Payload,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyIO")
            .field("payload", &P::NAME)
            .field("read", &READ)
            .field("write", &WRITE)
            .field("seek", &SEEK)
            .field("fileno", &FILENO)
            .field("obj", &self.obj)
            .finish()
    }
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> Clone
    for PyIO<P, READ, WRITE, SEEK, FILENO>
{
    fn clone(&self) -> Self {
        Python::attach(|py| Self {
            obj: self.obj.clone_ref(py),
            payload: PhantomData,
        })
    }
}

static IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static TEXT_IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static RAW_IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static BUFFERED_IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

fn type_name(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    Ok(obj.get_type().name()?.to_string())
}

/// The `bool` an `io.IOBase` query returned, or `None` if it raised or returned something else.
fn io_answer(answer: PyResult<Bound<'_, PyAny>>) -> Option<bool> {
    answer.ok()?.extract::<bool>().ok()
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    PyIO<P, READ, WRITE, SEEK, FILENO>
where
    P: Payload,
{
    /// Checks `obj` against the payload kind and capabilities and wraps it.
    ///
    /// Requires a callable method per capability, and rejects closed `io` objects, `io` objects
    /// of the other payload kind and ones whose `readable()`/`writable()`/`seekable()` say no.
    /// Objects outside the `io` hierarchy are taken at their word.
    pub fn py_new(obj: Bound<'_, PyAny>) -> Result<Self, Error> {
        Self::check_payload(&obj)?;
        Self::check_capabilities(&obj)?;
        Ok(Self {
            obj: obj.unbind(),
            payload: PhantomData,
        })
    }

    /// Like [`py_new`](Self::py_new), but attaches to Python itself.
    pub fn new(obj: Py<PyAny>) -> Result<Self, Error> {
        Python::attach(|py| Self::py_new(obj.into_bound(py)))
    }

    /// Only the `io` hierarchy is consulted: `mode` is unreliable, as `codecs` readers report
    /// their binary source's mode while returning `str`.
    fn check_payload(obj: &Bound<'_, PyAny>) -> Result<(), Error> {
        let py = obj.py();
        let is_text = obj.is_instance(TEXT_IO_BASE.import(py, "io", "TextIOBase")?)?;
        // Plain IOBase says nothing: SpooledTemporaryFile is only that, and can be text.
        let (wrong, why, hint) = if P::IS_TEXT {
            let is_binary = !is_text
                && (obj.is_instance(RAW_IO_BASE.import(py, "io", "RawIOBase")?)?
                    || obj.is_instance(BUFFERED_IO_BASE.import(py, "io", "BufferedIOBase")?)?);
            (
                is_binary,
                "it is an io.RawIOBase or io.BufferedIOBase",
                "Wrap it with io.TextIOWrapper(obj, encoding=...), which is where the choice of \
                 encoding belongs, or open the file in text mode.",
            )
        } else {
            (
                is_text,
                "it is an io.TextIOBase",
                "Pass obj.buffer to get at the bytes underneath, or open the file in binary mode. \
                 Decoding text and re-encoding it here would not round-trip the file's bytes.",
            )
        };
        if !wrong {
            return Ok(());
        }
        Err(Error::WrongKind {
            wanted: P::NAME,
            got: P::OTHER_NAME,
            type_name: type_name(obj)?,
            why,
            hint,
        })
    }

    fn check_capabilities(obj: &Bound<'_, PyAny>) -> Result<(), Error> {
        let py = obj.py();
        let require = |name: &Bound<'_, PyString>, method: &'static str| -> Result<(), Error> {
            match obj.getattr(name) {
                Ok(value) if value.is_callable() => Ok(()),
                Err(err) if !err.is_instance_of::<PyAttributeError>(py) => Err(err.into()),
                _ => Err(Error::MissingMethod {
                    type_name: type_name(obj)?,
                    method,
                }),
            }
        };
        // `closed` and the capability queries only have a defined meaning on IOBase.
        let is_io = obj.is_instance(IO_BASE.import(py, "io", "IOBase")?)?;
        if is_io && io_answer(obj.getattr(intern!(py, "closed"))) == Some(true) {
            return Err(PyValueError::new_err("I/O operation on closed file").into());
        }
        let require_capability = |query: &Bound<'_, PyString>,
                                  query_name: &'static str,
                                  method: &'static str,
                                  capability: &'static str|
         -> Result<(), Error> {
            if is_io && io_answer(obj.call_method0(query)) == Some(false) {
                return Err(Error::RefusesCapability {
                    type_name: type_name(obj)?,
                    query: query_name,
                    method,
                    capability,
                });
            }
            Ok(())
        };

        if READ {
            require(intern!(py, "read"), "read")?;
            require_capability(intern!(py, "readable"), "readable", "read", "READ")?;
        }
        if WRITE {
            // `flush` is optional: it is called only if present.
            require(intern!(py, "write"), "write")?;
            require_capability(intern!(py, "writable"), "writable", "write", "WRITE")?;
        }
        if SEEK {
            require(intern!(py, "seek"), "seek")?;
            require_capability(intern!(py, "seekable"), "seekable", "seek", "SEEK")?;
            if P::IS_TEXT {
                // Text streams only seek to cookies from tell().
                require(intern!(py, "tell"), "tell")?;
            }
        }
        if FILENO {
            require(intern!(py, "fileno"), "fileno")?;
        }
        Ok(())
    }

    /// Borrows the file with a token, so operations stop attaching for themselves.
    pub fn bind<'py>(&self, py: Python<'py>) -> BoundPyIO<'py, P, READ, WRITE, SEEK, FILENO> {
        BoundPyIO {
            obj: self.obj.bind(py).clone(),
            payload: PhantomData,
        }
    }

    /// Consuming form of [`bind`](Self::bind).
    pub fn into_bound<'py>(self, py: Python<'py>) -> BoundPyIO<'py, P, READ, WRITE, SEEK, FILENO> {
        BoundPyIO {
            obj: self.obj.into_bound(py),
            payload: PhantomData,
        }
    }

    /// The wrapped object.
    pub fn as_py_object(&self) -> &Py<PyAny> {
        &self.obj
    }

    /// Unwraps to the underlying object, discarding the static guarantees.
    pub fn into_py_object(self) -> Py<PyAny> {
        self.obj
    }
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool> PyIO<P, READ, WRITE, SEEK, true>
where
    P: Payload,
{
    /// The object's file descriptor. Unlike `AsRawFd`, this returns an error rather than
    /// panicking when `fileno()` raises.
    pub fn fileno(&self) -> std::io::Result<i32> {
        Python::attach(|py| self.bind(py).fileno())
    }
}

/// A [`PyIO`] tied to a `Python<'py>` token, so operations do not attach for themselves.
///
/// Like `Bound` versus `Py` in `pyo3`, this cannot leave the thread or outlive the token:
///
/// ```rust,no_run
/// use pyo3::prelude::*;
/// use pyo3_io::PyBinaryRead;
/// use std::io::Read;
///
/// #[pyfunction]
/// fn count(py: Python<'_>, file: PyBinaryRead) -> PyResult<usize> {
///     let mut file = file.into_bound(py); // no attaching per read from here on
///     let mut sink = Vec::new();
///     Ok(file.read_to_end(&mut sink)?)
/// }
/// ```
pub struct BoundPyIO<
    'py,
    P,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> {
    obj: Bound<'py, PyAny>,
    payload: PhantomData<fn() -> P>,
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> std::fmt::Debug
    for BoundPyIO<'_, P, READ, WRITE, SEEK, FILENO>
where
    P: Payload,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundPyIO")
            .field("payload", &P::NAME)
            .field("obj", &self.obj)
            .finish()
    }
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> Clone
    for BoundPyIO<'_, P, READ, WRITE, SEEK, FILENO>
{
    fn clone(&self) -> Self {
        Self {
            obj: self.obj.clone(),
            payload: PhantomData,
        }
    }
}

impl<'py, P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    BoundPyIO<'py, P, READ, WRITE, SEEK, FILENO>
where
    P: Payload,
{
    /// The wrapped object.
    pub fn as_py_object(&self) -> &Bound<'py, PyAny> {
        &self.obj
    }

    /// Releases the token, giving back a [`PyIO`] that can cross threads again.
    pub fn unbind(self) -> PyIO<P, READ, WRITE, SEEK, FILENO> {
        PyIO {
            obj: self.obj.unbind(),
            payload: PhantomData,
        }
    }
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool>
    BoundPyIO<'_, P, READ, WRITE, SEEK, true>
where
    P: Payload,
{
    /// The object's file descriptor. See [`PyIO::fileno`].
    pub fn fileno(&self) -> std::io::Result<i32> {
        let py = self.obj.py();
        Ok(self.obj.call_method0(intern!(py, "fileno"))?.extract()?)
    }
}

impl<'py, P, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    FromPyObject<'_, 'py> for PyIO<P, READ, WRITE, SEEK, FILENO>
where
    P: Payload,
{
    type Error = PyErr;

    #[cfg(feature = "experimental-inspect")]
    const INPUT_TYPE: pyo3::inspect::PyStaticExpr =
        introspection::protocol_hint(P::IS_TEXT, READ, WRITE, SEEK, FILENO);

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(Self::py_new(obj.as_any().clone())?)
    }
}
