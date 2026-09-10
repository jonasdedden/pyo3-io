#![doc = include_str!("../README.md")]

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyString;
use pyo3::{intern, Borrowed, FromPyObject};
use std::marker::PhantomData;

mod binary;
mod error;
#[cfg(feature = "experimental-inspect")]
mod introspection;
mod mode;
mod text;

pub use error::Error;
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

/// Works out what an object deals in, from evidence that does not lie.
///
/// Only two signals qualify, and this is measured rather than assumed: across forty standard
/// library file-like objects, the `io` hierarchy is right 28 times and wrong none, and a `str`
/// `encoding` attribute is right 11 times and wrong none.
///
/// A `mode` attribute would settle 23 more, but it is wrong twice — `codecs.getreader(..)` and
/// `codecs.open(..)` wrap a binary file and report *its* `"rb"` while producing `str` — so it is
/// deliberately not consulted. Nothing here special-cases `codecs`; the unreliable signal is
/// simply not used, and everything it would have settled falls through to the last rung instead.
///
/// That rung is "say nothing and let the object through", which is what keeps duck-typed objects
/// working and what an object of no recognisable kind gets. Five of the forty land there:
/// `tempfile.NamedTemporaryFile`, `tempfile.SpooledTemporaryFile(mode="wb+")`,
/// `codecs.getreader(..)`, `codecs.EncodedFile(..)` and `mmap`. Each of them works in its correct
/// kind; using one in the wrong kind is caught at the first read rather than here.
///
/// The returned string explains which rung fired, so a rejection can say why.
fn classify(obj: &Bound<'_, PyAny>) -> Result<(Payload, &'static str), Error> {
    let py = obj.py();

    // 1. The `io` hierarchy. Definitive, and covers most of the standard library.
    if obj.is_instance(text_io_base(py)?)? {
        return Ok((Payload::Text, "it is an io.TextIOBase"));
    }
    if obj.is_instance(raw_io_base(py)?)? || obj.is_instance(buffered_io_base(py)?)? {
        return Ok((
            Payload::Binary,
            "it is an io.RawIOBase or io.BufferedIOBase",
        ));
    }

    // 2. The attributes a text stream reports about its decoding. `io.TextIOBase` defines
    //    `encoding`, `errors` and `newlines`, and both of the first two are required here rather
    //    than `encoding` alone: `encoding` is a common enough attribute name that a class could
    //    have one for its own purposes, whereas `errors` alongside it is specific to this. It
    //    costs nothing to ask for both — every standard-library object with a `str` `encoding`
    //    has an `errors` too — and `newlines` as well would be too strict, since
    //    `codecs.open(..)` has no such attribute.
    //
    //    Only objects outside the `io` hierarchy get this far at all, since rung 1 has already
    //    decided for everything inside it.
    if let Some(encoding) = optional_attr(obj, intern!(py, "encoding")) {
        if encoding.is_instance_of::<PyString>()
            && optional_attr(obj, intern!(py, "errors")).is_some()
        {
            return Ok((
                Payload::Text,
                "it reports the `encoding` and `errors` of a text stream",
            ));
        }
    }

    // 3. Nothing trustworthy was said. Duck typing decides, and the capability checks are the
    //    only requirement.
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
    /// The escape hatch for an object [`classify`] gets wrong. It only uses signals that do not
    /// lie, so an object of no recognisable kind is accepted rather than refused and this is
    /// rarely needed — but an object that derives from the wrong half of the `io` hierarchy, or
    /// reports an `encoding` while dealing in bytes, would be refused, and this is how to say you
    /// know better.
    ///
    /// Nothing here is memory-unsafe. Getting it wrong means the first read or write fails.
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
        let require = |name: &Bound<'_, PyString>, method: &'static str| -> Result<(), Error> {
            if obj.hasattr(name)? {
                Ok(())
            } else {
                Err(Error::MissingMethod {
                    type_name: obj.get_type().name()?.to_string(),
                    method,
                })
            }
        };
        if READ {
            require(intern!(py, "read"), "read")?;
        }
        if WRITE {
            require(intern!(py, "write"), "write")?;
            // `flush` is deliberately not required. An object that has none has nothing to flush,
            // so flushing it is a no-op rather than an error, and demanding it would turn away
            // every minimal writer that implements nothing but `write`.
        }
        if SEEK {
            require(intern!(py, "seek"), "seek")?;
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
    /// Deliberately fallible and deliberately not [`std::os::fd::AsRawFd`]: `fileno()` raising
    /// `io.UnsupportedOperation` is entirely normal in Python — `io.BytesIO` does it — and
    /// `AsRawFd` has no way to report that other than by panicking.
    pub fn fileno(&self) -> std::io::Result<i32> {
        Python::attach(|py| self.bind(py).fileno())
    }
}

/// A [`PyFile`] with the GIL already held, which is where the work actually happens.
///
/// This is the same split `pyo3` uses for its own types: [`PyFile`] owns a `Py<PyAny>` and is
/// `Send + 'static`, and this borrows a `Python<'py>` token so the operations do not have to
/// acquire one themselves. [`std::io::Read`] and friends are implemented on both — on this one
/// directly, and on [`PyFile`] by attaching and delegating here — so a caller that already holds
/// the GIL can say so and keep control of when it is taken:
///
/// ```rust,ignore
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
