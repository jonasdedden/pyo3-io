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
mod file_types;
#[cfg(feature = "experimental-inspect")]
mod introspection;
mod mode;
mod text;
mod write;

pub use error::Error;
pub use mode::{Binary, Mode, Text};
pub use text::TextPosition;

mod sealed {
    pub trait Sealed {}
}

/// A Python file-like object dealing in `bytes`, whose capabilities are part of its type.
///
/// The four const parameters are the operations the object must support, in the order `READ`,
/// `WRITE`, `SEEK`, `FILENO`. Prefer the named aliases — [`BinaryRead`], [`BinaryReadSeek`] and
/// so on — over spelling them out.
///
/// Only the capabilities that were asked for exist on the value, so a read-only file cannot be
/// written to, and it can never be confused with a [`PyTextFile`]. Both are compile errors rather
/// than runtime ones.
///
/// # API guide
///
/// The trait implementations below are conditional: `READ` enables [`std::io::Read`],
/// `WRITE` enables [`std::io::Write`], and `SEEK` enables [`std::io::Seek`].
/// Import the corresponding trait to call its methods.
///
/// Shared operations are documented on [`PyFile`]: [`new`](PyFile::new),
/// [`py_new`](PyFile::py_new), [`bind`](PyFile::bind), [`into_bound`](PyFile::into_bound),
/// and object access. `FILENO` enables [`fileno`](PyFile::fileno).
/// `Clone`, `Debug` and `FromPyObject` are also implemented through that shared type.
///
/// Start with [`BinaryRead`], [`BinaryWrite`] or [`BinaryReadSeek`], or browse the
/// [complete alias catalog](aliases). The attached reference is [`BoundPyBinaryFile`].
pub type PyBinaryFile<const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> =
    PyFile<Binary, READ, WRITE, SEEK, FILENO>;

/// A Python file-like object dealing in `str`, whose capabilities are part of its type.
///
/// As [`PyBinaryFile`], except that Python counts a text `read(size)` in characters, so this
/// deliberately does not implement [`std::io::Read`]: it has a character API instead, and seeks
/// by the opaque cookies `tell()` produces rather than by byte offsets.
///
/// # API guide
///
/// The inherent implementations below are conditional: `READ` enables character reads,
/// `WRITE` enables character writes and flushing, and `SEEK` enables opaque-cookie seeking.
/// No text file implements `std::io::Read`, `Write` or `Seek`.
///
/// Shared operations are documented on [`PyFile`]: [`new`](PyFile::new),
/// [`py_new`](PyFile::py_new), [`bind`](PyFile::bind), [`into_bound`](PyFile::into_bound),
/// and object access. `FILENO` enables [`fileno`](PyFile::fileno).
/// `Clone`, `Debug` and `FromPyObject` are also implemented through that shared type.
///
/// Start with [`TextRead`], [`TextWrite`] or [`TextReadSeek`], or browse the
/// [complete alias catalog](aliases). The attached reference is [`BoundPyTextFile`].
pub type PyTextFile<const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> =
    PyFile<Text, READ, WRITE, SEEK, FILENO>;

/// A binary file tied to an attached Python token.
///
/// Obtain it with [`PyFile::bind`] or [`PyFile::into_bound`]. The trait implementations
/// below use the same capability flags as [`PyBinaryFile`], without attaching per call.
/// Shared [`as_py_object`](BoundFile::as_py_object), [`unbind`](BoundFile::unbind) and
/// conditional [`fileno`](BoundFile::fileno) methods are documented on [`BoundFile`].
/// See the [bound alias catalog](aliases::bound) for concrete signatures.
pub type BoundPyBinaryFile<
    'py,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> = BoundFile<'py, Binary, READ, WRITE, SEEK, FILENO>;

/// A text file tied to an attached Python token.
///
/// Obtain it with [`PyFile::bind`] or [`PyFile::into_bound`]. The inherent implementations
/// below use the same capability flags as [`PyTextFile`], without attaching per call.
/// Shared [`as_py_object`](BoundFile::as_py_object), [`unbind`](BoundFile::unbind) and
/// conditional [`fileno`](BoundFile::fileno) methods are documented on [`BoundFile`].
/// See the [bound alias catalog](aliases::bound) for concrete signatures.
pub type BoundPyTextFile<
    'py,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> = BoundFile<'py, Text, READ, WRITE, SEEK, FILENO>;

pub mod aliases;

// Keep every existing root import valid, but put the complete catalogs in their own pages.
#[doc(hidden)]
pub use aliases::bound::*;
#[doc(hidden)]
pub use aliases::*;
#[doc(inline)]
pub use aliases::{BinaryRead, BinaryReadSeek, BinaryWrite, TextRead, TextReadSeek, TextWrite};

/// Shared construction, binding and object access for owned binary and text files.
///
/// Start with [`BinaryRead`], [`BinaryWrite`], [`TextRead`] or another [named alias](aliases),
/// rather than spelling this type's parameters yourself. All owned aliases share the
/// constructors and binding methods documented here; `M` is sealed to [`Binary`] and [`Text`].
///
/// Use [`PyBinaryFile`] for the binary I/O reference and [`PyTextFile`] for the text API.
/// [`bind`](Self::bind) and [`into_bound`](Self::into_bound) produce the attached [`BoundFile`].
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

// PyOnceLock detaches while waiting for initialization, avoiding interpreter/lock deadlocks.
static IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static TEXT_IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static RAW_IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();
static BUFFERED_IO_BASE: PyOnceLock<Py<PyType>> = PyOnceLock::new();

/// What an object says about the kind of payload it deals in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Payload {
    /// Declares a binary I/O contract.
    Binary,
    /// Declares a text I/O contract.
    Text,
    /// The object said nothing either way, so it is taken at its word.
    Unknown,
}

/// Classifies the declared I/O hierarchy, without probing read or write.
///
/// Unknown ducks remain acceptable. Attributes such as `mode` and `encoding` are not reliable
/// payload contracts: codecs wrappers can expose a binary mode while returning text.
/// Python instance checks can execute custom code, and subclasses can violate their contracts.
fn classify(obj: &Bound<'_, PyAny>) -> Result<(Payload, &'static str), Error> {
    let py = obj.py();
    // IOBase alone is not binary: SpooledTemporaryFile can be a text stream.
    if obj.is_instance(TEXT_IO_BASE.import(py, "io", "TextIOBase")?)? {
        return Ok((Payload::Text, "it is an io.TextIOBase"));
    }
    if obj.is_instance(RAW_IO_BASE.import(py, "io", "RawIOBase")?)?
        || obj.is_instance(BUFFERED_IO_BASE.import(py, "io", "BufferedIOBase")?)?
    {
        return Ok((
            Payload::Binary,
            "it is an io.RawIOBase or io.BufferedIOBase",
        ));
    }

    Ok((Payload::Unknown, "it is duck typed"))
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    PyFile<M, READ, WRITE, SEEK, FILENO>
where
    M: Mode,
{
    /// Checks `obj` against the payload kind and capabilities and wraps it.
    ///
    /// Requires callable methods, including ones supplied dynamically by `__getattr__`.
    /// Known opposite-kind I/O classes, closed I/O objects, and explicit capability refusals
    /// are rejected. Unknown duck-typed payload kinds are accepted.
    ///
    /// These are best-effort checks, not proof that later I/O succeeds. Attribute lookup and
    /// capability queries may run Python code, and the object can change after construction.
    pub fn py_new(obj: Bound<'_, PyAny>) -> Result<Self, Error> {
        Self::check_mode(&obj)?;
        Self::check_capabilities(&obj)?;
        Ok(Self {
            obj: obj.unbind(),
            mode: PhantomData,
        })
    }

    /// Same as [`py_new`](Self::py_new) but takes and re-attaches to acquire the GIL itself.
    pub fn new(obj: Py<PyAny>) -> Result<Self, Error> {
        Python::attach(|py| Self::py_new(obj.into_bound(py)))
    }

    /// Wraps `obj` without checking the payload kind, keeping the capability checks.
    ///
    /// For an object that inherits from the wrong payload hierarchy. This still checks
    /// callable methods, closed state and capability refusals. Actual read results and write
    /// counts remain validated; this is not a memory-unsafe operation.
    pub fn py_new_unchecked(obj: Bound<'_, PyAny>) -> Result<Self, Error> {
        Self::check_capabilities(&obj)?;
        Ok(Self {
            obj: obj.unbind(),
            mode: PhantomData,
        })
    }

    fn check_mode(obj: &Bound<'_, PyAny>) -> Result<(), Error> {
        let (payload, why) = classify(obj)?;
        let wrong = match (M::IS_TEXT, payload) {
            // Agrees, or the object said nothing and is taken at its word.
            (_, Payload::Unknown) | (true, Payload::Text) | (false, Payload::Binary) => {
                return Ok(())
            }
            (false, Payload::Text) => Error::WrongKind {
                wanted: "binary",
                got: "text",
                type_name: obj.get_type().name()?.to_string(),
                why,
                hint: "Pass obj.buffer to get at the bytes underneath, or open the file in binary \
                       mode. Decoding text and re-encoding it here would not round-trip the \
                       file's bytes.",
            },
            (true, Payload::Binary) => Error::WrongKind {
                wanted: "text",
                got: "binary",
                type_name: obj.get_type().name()?.to_string(),
                why,
                hint: "Wrap it with io.TextIOWrapper(obj, encoding=...), which is where the \
                       choice of encoding belongs, or open the file in text mode.",
            },
        };
        Err(wrong)
    }

    fn check_capabilities(obj: &Bound<'_, PyAny>) -> Result<(), Error> {
        let py = obj.py();
        let missing = |method: &'static str| -> Result<Error, PyErr> {
            Ok(Error::MissingMethod {
                type_name: obj.get_type().name()?.to_string(),
                method,
            })
        };
        let require = |name: &Bound<'_, PyString>, method: &'static str| -> Result<(), Error> {
            match obj.getattr(name) {
                Ok(value) if value.is_callable() => Ok(()),
                Ok(_) => Err(missing(method)?),
                Err(err) if err.is_instance_of::<PyAttributeError>(py) => Err(missing(method)?),
                Err(err) => Err(err.into()),
            }
        };
        let is_io = obj.is_instance(IO_BASE.import(py, "io", "IOBase")?)?;
        // Only IOBase gives these attributes a defined meaning. Do not inspect unrelated
        // ducks' state/query attributes. A raise or a non-bool is an unknown answer.
        if is_io
            && obj
                .getattr(intern!(py, "closed"))
                .ok()
                .and_then(|answer| answer.extract::<bool>().ok())
                == Some(true)
        {
            return Err(PyValueError::new_err("I/O operation on closed file").into());
        }
        let require_capability = |query: &Bound<'_, PyString>,
                                  query_name: &'static str,
                                  method: &'static str,
                                  capability: &'static str|
         -> Result<(), Error> {
            if is_io
                && obj
                    .call_method0(query)
                    .ok()
                    .and_then(|answer| answer.extract::<bool>().ok())
                    == Some(false)
            {
                Err(Error::RefusesCapability {
                    type_name: obj.get_type().name()?.to_string(),
                    query: query_name,
                    method,
                    capability,
                })
            } else {
                Ok(())
            }
        };

        if READ {
            require(intern!(py, "read"), "read")?;
            require_capability(intern!(py, "readable"), "readable", "read", "READ")?;
        }
        if WRITE {
            require(intern!(py, "write"), "write")?;
            require_capability(intern!(py, "writable"), "writable", "write", "WRITE")?;
            // `flush` is deliberately not required. An object that has none has nothing to flush,
            // so flushing it is a no-op rather than an error, and demanding it would turn away
            // every minimal writer that implements nothing but `write`.
        }
        if SEEK {
            require(intern!(py, "seek"), "seek")?;
            require_capability(intern!(py, "seekable"), "seekable", "seek", "SEEK")?;
            if M::IS_TEXT {
                // Text streams only accept opaque cookies, which come from tell()
                require(intern!(py, "tell"), "tell")?;
            }
        }
        if FILENO {
            require(intern!(py, "fileno"), "fileno")?;
        }
        Ok(())
    }

    /// Borrows the file with a token, so the operations stop attaching for themselves.
    pub fn bind<'py>(&self, py: Python<'py>) -> BoundFile<'py, M, READ, WRITE, SEEK, FILENO> {
        BoundFile {
            obj: self.obj.bind(py).clone(),
            mode: PhantomData,
        }
    }

    /// Consuming form of [`bind`](Self::bind).
    pub fn into_bound<'py>(self, py: Python<'py>) -> BoundFile<'py, M, READ, WRITE, SEEK, FILENO> {
        BoundFile {
            obj: self.obj.into_bound(py),
            mode: PhantomData,
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

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool> PyFile<M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// The object's file descriptor.
    ///
    /// Unlike the Unix `AsRawFd` compatibility trait, this reports Python failures rather
    /// than panicking. A number is not a borrowed-descriptor lifetime guarantee.
    pub fn fileno(&self) -> std::io::Result<i32> {
        Python::attach(|py| self.bind(py).fileno())
    }
}

/// Shared object access and token release for attached binary and text files.
///
/// Use [`BoundPyBinaryFile`] for the binary I/O reference and [`BoundPyTextFile`] for
/// the text API. The [bound alias catalog](aliases::bound) lists concrete signatures.
/// See [`PyFile`] for shared construction and binding.
///
/// This is the same split `pyo3` uses for its own types: the detached form owns a `Py<PyAny>` and
/// is `Send + 'static`, and this borrows a `Python<'py>` token so the operations do not have to
/// acquire one themselves. [`std::io::Read`] and friends are implemented on both — on this one
/// directly, and on the detached form by attaching and delegating here — so a caller that already
/// holds the GIL can say so and keep control of when it is taken:
///
/// ```rust,no_run
/// use pyo3::prelude::*;
/// use pyo3_typed_io::BinaryRead;
/// use std::io::Read;
///
/// #[pyfunction]
/// fn count(py: Python<'_>, file: BinaryRead) -> PyResult<usize> {
///     let mut file = file.into_bound(py);   // no attaching per read from here on
///     let mut sink = Vec::new();
///     Ok(file.read_to_end(&mut sink)?)
/// }
/// ```
///
/// Unlike [`PyFile`] this cannot leave the thread or outlive the token, which is the trade: see
/// the crate documentation.
pub struct BoundFile<
    'py,
    M,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> {
    obj: Bound<'py, PyAny>,
    mode: PhantomData<fn() -> M>,
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> std::fmt::Debug
    for BoundFile<'_, M, READ, WRITE, SEEK, FILENO>
where
    M: Mode,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundFile")
            .field("mode", &M::NAME)
            .field("obj", &self.obj)
            .finish()
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> Clone
    for BoundFile<'_, M, READ, WRITE, SEEK, FILENO>
{
    fn clone(&self) -> Self {
        Self {
            obj: self.obj.clone(),
            mode: PhantomData,
        }
    }
}

impl<'py, M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    BoundFile<'py, M, READ, WRITE, SEEK, FILENO>
where
    M: Mode,
{
    /// The wrapped object.
    pub fn as_py_object(&self) -> &Bound<'py, PyAny> {
        &self.obj
    }

    /// Releases the token, giving back a [`PyFile`] that can cross threads again.
    pub fn unbind(self) -> PyFile<M, READ, WRITE, SEEK, FILENO> {
        PyFile {
            obj: self.obj.unbind(),
            mode: PhantomData,
        }
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool>
    BoundFile<'_, M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// The object's file descriptor. See [`PyFile::fileno`].
    pub fn fileno(&self) -> std::io::Result<i32> {
        let py = self.obj.py();
        (|| -> Result<i32, Error> {
            Ok(self
                .obj
                .call_method0(intern!(py, "fileno"))?
                .extract::<i32>()?)
        })()
        .map_err(Into::into)
    }
}

/// A [`PyFile`] extracted without the payload-kind check, keeping the capability checks.
///
/// The escape hatch for an object that misidentifies itself, in a form that can still appear in a
/// `#[pyfunction]` signature — it carries the same type stub protocol as the file it wraps, so
/// using it costs nothing in the annotation:
///
/// ```rust,no_run
/// use pyo3::prelude::*;
/// use pyo3_typed_io::{TextRead, Unchecked};
///
/// #[pyfunction]
/// fn read_it(mut source: Unchecked<TextRead>) -> PyResult<String> {
///     Ok(source.read_to_string()?)
/// }
/// // def read_it(source: SupportsTextRead) -> str: ...
/// ```
///
/// See [`PyFile::py_new_unchecked`] for when that is needed.
#[derive(Debug, Clone)]
pub struct Unchecked<T>(T);

impl<T> Unchecked<T> {
    /// The file underneath.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> core::ops::Deref for Unchecked<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> core::ops::DerefMut for Unchecked<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<'py, M, const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool>
    FromPyObject<'_, 'py> for Unchecked<PyFile<M, READ, WRITE, SEEK, FILENO>>
where
    M: Mode,
{
    type Error = PyErr;

    // Deliberately the same protocol as the checked form: skipping the runtime check says nothing
    // about what the object has to provide.
    #[cfg(feature = "experimental-inspect")]
    const INPUT_TYPE: pyo3::inspect::PyStaticExpr =
        introspection::protocol_hint(M::IS_TEXT, READ, WRITE, SEEK, FILENO);

    fn extract(obj: Borrowed<'_, 'py, PyAny>) -> Result<Self, Self::Error> {
        Ok(Self(PyFile::py_new_unchecked(obj.as_any().clone())?))
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
        Ok(Self::py_new(obj.as_any().clone())?)
    }
}
