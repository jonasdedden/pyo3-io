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
    module: &str,
    name: &str,
) -> PyResult<&'py Bound<'py, PyAny>> {
    cell.get_or_try_init(py, || Ok(py.import(module)?.getattr(name)?.unbind()))
        .map(|obj| obj.bind(py))
}

/// Declares a lazily imported, cached type object.
macro_rules! cached {
    ($name:ident, $module:literal, $attr:literal) => {
        fn $name<'py>(py: Python<'py>) -> PyResult<&'py Bound<'py, PyAny>> {
            static CELL: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
            cached_type(py, &CELL, $module, $attr)
        }
    };
}

cached!(text_io_base, "io", "TextIOBase");
cached!(raw_io_base, "io", "RawIOBase");
cached!(buffered_io_base, "io", "BufferedIOBase");
cached!(codecs_stream_reader, "codecs", "StreamReader");
cached!(codecs_stream_writer, "codecs", "StreamWriter");
cached!(codecs_stream_reader_writer, "codecs", "StreamReaderWriter");

/// What an object says about the kind of payload it deals in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Payload {
    /// Definitely `bytes`.
    Binary,
    /// Definitely `str`.
    Text,
    /// The object said nothing either way, so it is taken at its word.
    Unknown,
}

/// Works out what an object deals in, preferring the most explicit evidence available.
///
/// The ladder matters, because the cheap signals are the unreliable ones. `codecs.getreader(..)`
/// wraps a binary file and reports *its* `mode` of `"rb"` while producing `str`, so anything that
/// consulted `mode` first would get it exactly backwards. Each rung is only reached when the ones
/// above it had nothing to say, and the last rung is "say nothing and let the object through",
/// which is what keeps duck-typed objects working.
///
/// The returned string explains which rung fired, so a rejection can say why.
fn classify(obj: &Bound<'_, PyAny>) -> PyResult<(Payload, &'static str)> {
    let py = obj.py();

    // 1. The `io` hierarchy. Definitive, and covers nearly every object from the standard library.
    if obj.is_instance(text_io_base(py)?)? {
        return Ok((Payload::Text, "it is an io.TextIOBase"));
    }
    if obj.is_instance(raw_io_base(py)?)? || obj.is_instance(buffered_io_base(py)?)? {
        return Ok((
            Payload::Binary,
            "it is an io.RawIOBase or io.BufferedIOBase",
        ));
    }

    // 2. The `codecs` stream wrappers. Outside the `io` hierarchy entirely, and they decode to
    //    `str`, so they have to be recognised before `mode` is consulted.
    for codecs_type in [
        codecs_stream_reader(py)?,
        codecs_stream_writer(py)?,
        codecs_stream_reader_writer(py)?,
    ] {
        if obj.is_instance(codecs_type)? {
            return Ok((Payload::Text, "it is a codecs stream wrapper"));
        }
    }

    // 3. A `str` encoding. Only a text stream has one to report.
    if let Some(encoding) = optional_attr(obj, intern!(py, "encoding")) {
        if encoding.is_instance_of::<PyString>() {
            return Ok((Payload::Text, "it has a str `encoding` attribute"));
        }
    }

    // 4. A self-declared `mode`. This is what `tempfile.SpooledTemporaryFile` leaves to go on,
    //    since it derives from `io.IOBase` without saying which half.
    if let Some(mode) = optional_attr(obj, intern!(py, "mode")) {
        if let Ok(mode) = mode.extract::<std::borrow::Cow<'_, str>>() {
            return Ok(if mode.contains('b') {
                (Payload::Binary, "its `mode` attribute contains 'b'")
            } else {
                (Payload::Text, "its `mode` attribute does not contain 'b'")
            });
        }
    }

    // 5. Nothing said. Duck typing decides, and the capability checks are the only requirement.
    Ok((Payload::Unknown, "it is duck typed"))
}

/// An attribute if it is there, and `None` for anything else, including a property that raises.
fn optional_attr<'py>(
    obj: &Bound<'py, PyAny>,
    name: &Bound<'py, PyString>,
) -> Option<Bound<'py, PyAny>> {
    obj.getattr(name).ok()
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

    /// Wraps `obj` without checking the payload kind, keeping the capability checks.
    ///
    /// The escape hatch for an object that misidentifies itself. The only way that happens in the
    /// standard library is `codecs.getreader(..)` and `codecs.open(..)`, which report the wrapped
    /// file's `mode` of `"rb"` while producing `str`, and both are recognised by name. A
    /// third-party wrapper doing the same thing without deriving from the `codecs` classes would
    /// be classified by its `mode` and refused; this is how to say you know better.
    ///
    /// Nothing here is memory-unsafe. Getting it wrong means the first read or write fails.
    pub fn py_new_unchecked(obj: Bound<'_, PyAny>) -> PyResult<Self> {
        Self::check_capabilities(&obj)?;
        Ok(Self {
            obj: obj.unbind(),
            mode: PhantomData,
        })
    }

    fn check_mode(obj: &Bound<'_, PyAny>) -> PyResult<()> {
        let (payload, why) = classify(obj)?;
        let name = obj.get_type().name()?;
        match (M::IS_TEXT, payload) {
            // Agrees, or the object said nothing and is taken at its word.
            (_, Payload::Unknown) | (true, Payload::Text) | (false, Payload::Binary) => Ok(()),
            (false, Payload::Text) => Err(PyTypeError::new_err(format!(
                "expected a binary file-like object, got the text object {name} ({why}). \
                 Pass obj.buffer to get at the bytes underneath, or open the file in binary mode. \
                 Decoding text and re-encoding it here would not round-trip the file's bytes."
            ))),
            (true, Payload::Binary) => Err(PyTypeError::new_err(format!(
                "expected a text file-like object, got the binary object {name} ({why}). \
                 Wrap it with io.TextIOWrapper(obj, encoding=...), which is where the choice \
                 of encoding belongs, or open the file in text mode."
            ))),
        }
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
            // `flush` is deliberately not required. An object that has none has nothing to flush,
            // so flushing it is a no-op rather than an error, and demanding it would turn away
            // every minimal writer that implements nothing but `write`.
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

/// A [`PyFile`] extracted without the payload-kind check, keeping the capability checks.
///
/// The escape hatch for an object that misidentifies itself, in a form that can still appear in a
/// `#[pyfunction]` signature — it carries the same type stub protocol as the file it wraps, so
/// using it costs nothing in the annotation:
///
/// ```rust,ignore
/// #[pyfunction]
/// fn read_it(source: Unchecked<TextRead>) -> PyResult<String> { .. }
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
        PyFile::py_new_unchecked(obj.as_any().clone()).map(Self)
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
