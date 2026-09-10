#![doc = include_str!("../README.md")]

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyString;
use pyo3::{intern, Borrowed, FromPyObject};
use std::marker::PhantomData;

mod binary;
#[cfg(feature = "experimental-inspect")]
mod introspection;
mod mode;
mod text;

pub use mode::{Binary, Mode, Text};

mod sealed {
    pub trait Sealed {}
}

include!(concat!(env!("OUT_DIR"), "/aliases.rs"));

/// A Python file-like object whose payload kind and capabilities are part of its Rust type.
///
/// `M` is [`Binary`] or [`Text`]. The four const parameters are the operations the object must
/// support, in the order `READ`, `WRITE`, `SEEK`, `FILENO`. Prefer the aliases —
/// [`BinaryRead`], [`TextReadSeek`] and so on — over spelling the parameters out.
///
/// Only the capabilities that were asked for exist on the value, so a read-only file cannot be
/// written to and a text file cannot be handed to something expecting bytes. Both are compile
/// errors rather than runtime ones.
pub struct PyFile<M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> {
    obj: Py<PyAny>,
    // `fn() -> M` rather than `M` so the auto traits and variance come from `Py<PyAny>` alone
    mode: PhantomData<fn() -> M>,
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> std::fmt::Debug
    for PyFile<M, READ, WRITE, SEEK, FILENO>
where
    M: Mode,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PyFile")
            .field("mode", &M::NAME)
            .field("read", &READ)
            .field("write", &WRITE)
            .field("seek", &SEEK)
            .field("fileno", &FILENO)
            .field("obj", &self.obj)
            .finish()
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> Clone
    for PyFile<M, READ, WRITE, SEEK, FILENO>
{
    fn clone(&self) -> Self {
        Python::attach(|py| Self {
            obj: self.obj.clone_ref(py),
            mode: PhantomData,
        })
    }
}

fn cached_type<'py>(
    py: Python<'py>,
    cell: &'static PyOnceLock<Py<PyAny>>,
    name: &str,
) -> PyResult<&'py Bound<'py, PyAny>> {
    cell.get_or_try_init(py, || Ok(py.import("io")?.getattr(name)?.unbind()))
        .map(|obj| obj.bind(py))
}

/// `io.TextIOBase`, the base of every stream whose `read` yields `str`.
fn text_io_base<'py>(py: Python<'py>) -> PyResult<&'py Bound<'py, PyAny>> {
    static CELL: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    cached_type(py, &CELL, "TextIOBase")
}

/// `io.RawIOBase`, one of the two bases of streams whose `read` yields `bytes`.
fn raw_io_base<'py>(py: Python<'py>) -> PyResult<&'py Bound<'py, PyAny>> {
    static CELL: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    cached_type(py, &CELL, "RawIOBase")
}

/// `io.BufferedIOBase`, the other one.
fn buffered_io_base<'py>(py: Python<'py>) -> PyResult<&'py Bound<'py, PyAny>> {
    static CELL: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
    cached_type(py, &CELL, "BufferedIOBase")
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    PyFile<M, READ, WRITE, SEEK, FILENO>
where
    M: Mode,
{
    /// Checks `obj` against the payload kind and capabilities and wraps it.
    ///
    /// The capability checks are `hasattr`, so a duck-typed object is as acceptable as a real
    /// file. The payload kind check only *rejects*: an object that is definitely of the wrong kind
    /// is refused here rather than failing confusingly on the first read, but an object that is
    /// neither an `io.TextIOBase` nor an `io.RawIOBase`/`io.BufferedIOBase` is taken at its word.
    pub fn py_new(obj: Bound<'_, PyAny>) -> PyResult<Self> {
        Self::check_mode(&obj)?;
        Self::check_capabilities(&obj)?;
        Ok(Self {
            obj: obj.unbind(),
            mode: PhantomData,
        })
    }

    /// Same as [`py_new`](Self::py_new) but takes and re-attaches to acquire the GIL itself.
    pub fn new(obj: Py<PyAny>) -> PyResult<Self> {
        Python::attach(|py| Self::py_new(obj.into_bound(py)))
    }

    fn check_mode(obj: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = obj.py();
        if M::IS_TEXT {
            let binary =
                obj.is_instance(raw_io_base(py)?)? || obj.is_instance(buffered_io_base(py)?)?;
            if binary {
                return Err(PyTypeError::new_err(format!(
                    "expected a text file-like object, got the binary object {}. \
                     Wrap it with io.TextIOWrapper(obj, encoding=...), which is where the choice \
                     of encoding belongs, or open the file in text mode.",
                    obj.get_type().name()?
                )));
            }
        } else if obj.is_instance(text_io_base(py)?)? {
            return Err(PyTypeError::new_err(format!(
                "expected a binary file-like object, got the text object {}. \
                 Pass obj.buffer to get at the bytes underneath, or open the file in binary mode. \
                 Decoding text and re-encoding it here would not round-trip the file's bytes.",
                obj.get_type().name()?
            )));
        }
        Ok(())
    }

    fn check_capabilities(obj: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = obj.py();
        let require = |name: &Bound<'_, PyString>| -> PyResult<()> {
            if obj.hasattr(name)? {
                Ok(())
            } else {
                Err(PyTypeError::new_err(format!(
                    "object of type {} has no .{}() method",
                    obj.get_type().name()?,
                    name
                )))
            }
        };
        if READ {
            require(intern!(py, "read"))?;
        }
        if WRITE {
            require(intern!(py, "write"))?;
            // Writing hands out a flush, so the object needs one whether or not it is ever called
            require(intern!(py, "flush"))?;
        }
        if SEEK {
            require(intern!(py, "seek"))?;
            if M::IS_TEXT {
                // Text streams only accept opaque cookies, which come from tell()
                require(intern!(py, "tell"))?;
            }
        }
        if FILENO {
            require(intern!(py, "fileno"))?;
        }
        Ok(())
    }

    /// The wrapped object.
    pub fn as_py_object(&self) -> &Py<PyAny> {
        &self.obj
    }

    /// Unwraps to the underlying object, discarding the static guarantees.
    pub fn into_py_object(self) -> Py<PyAny> {
        self.obj
    }

    fn call<'py, T>(
        &self,
        py: Python<'py>,
        f: impl FnOnce(&Bound<'py, PyAny>) -> PyResult<T>,
    ) -> std::io::Result<T> {
        f(self.obj.bind(py)).map_err(Into::into)
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool> PyFile<M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// The object's file descriptor.
    ///
    /// Deliberately fallible and deliberately not [`std::os::fd::AsRawFd`]: `fileno()` raising
    /// `io.UnsupportedOperation` is entirely normal in Python — `io.BytesIO` does it — and
    /// `AsRawFd` has no way to report that other than by panicking.
    pub fn fileno(&self) -> std::io::Result<i32> {
        Python::attach(|py| {
            self.call(py, |obj| {
                obj.call_method0(intern!(py, "fileno"))?.extract::<i32>()
            })
        })
    }
}

impl<'py, M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    FromPyObject<'_, 'py> for PyFile<M, READ, WRITE, SEEK, FILENO>
where
    M: Mode,
{
    type Error = PyErr;

    #[cfg(feature = "experimental-inspect")]
    const INPUT_TYPE: pyo3::inspect::PyStaticExpr =
        introspection::protocol_hint(M::IS_TEXT, READ, WRITE, SEEK, FILENO);

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        Self::py_new(obj.as_any().clone())
    }
}
