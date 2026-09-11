#![doc = include_str!("../README.md")]

use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::PyString;
use pyo3::{intern, Borrowed, FromPyObject};
use std::marker::PhantomData;

mod binary;
mod error;
#[cfg(unix)]
mod fd;
#[cfg(feature = "experimental-inspect")]
mod introspection;
mod mode;
mod text;

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
pub type PyBinaryFile<const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> =
    PyFile<Binary, READ, WRITE, SEEK, FILENO>;

/// A Python file-like object dealing in `str`, whose capabilities are part of its type.
///
/// As [`PyBinaryFile`], except that Python counts a text `read(size)` in characters, so this
/// deliberately does not implement [`std::io::Read`]: it has a character API instead, and seeks
/// by the opaque cookies `tell()` produces rather than by byte offsets.
pub type PyTextFile<const READ: bool, const WRITE: bool, const SEEK: bool, const FILENO: bool> =
    PyFile<Text, READ, WRITE, SEEK, FILENO>;

/// A [`PyBinaryFile`] with the GIL held. See [`BoundFile`].
pub type BoundPyBinaryFile<
    'py,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> = BoundFile<'py, Binary, READ, WRITE, SEEK, FILENO>;

/// A [`PyTextFile`] with the GIL held. See [`BoundFile`].
pub type BoundPyTextFile<
    'py,
    const READ: bool,
    const WRITE: bool,
    const SEEK: bool,
    const FILENO: bool,
> = BoundFile<'py, Text, READ, WRITE, SEEK, FILENO>;

mod aliases;
pub use aliases::*;

/// The shared implementation behind [`PyBinaryFile`] and [`PyTextFile`].
///
/// Not the public face of anything: `M` is sealed to [`Binary`] and [`Text`], and the two aliases
/// are what the documentation and the error messages talk about. It exists as one type because
/// construction, capability checking and the `Py`/`Bound` split are identical for both kinds.
#[doc(hidden)]
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

/// Declares a lazily imported type that may not exist on this Python version.
macro_rules! cached_optional {
    ($name:ident, $module:literal, $attr:literal) => {
        fn $name<'py>(py: Python<'py>) -> PyResult<Option<&'py Bound<'py, PyAny>>> {
            static CELL: PyOnceLock<Option<Py<PyAny>>> = PyOnceLock::new();
            let cell = CELL.get_or_try_init(py, || -> PyResult<_> {
                Ok(py.import($module)?.getattr($attr).ok().map(Bound::unbind))
            })?;
            Ok(cell.as_ref().map(|obj| obj.bind(py)))
        }
    };
}

cached!(io_base, "io", "IOBase");
cached!(text_io_base, "io", "TextIOBase");
cached!(raw_io_base, "io", "RawIOBase");
cached!(buffered_io_base, "io", "BufferedIOBase");
// New in Python 3.14, so absent on everything older.
cached_optional!(io_reader, "io", "Reader");
cached_optional!(io_writer, "io", "Writer");

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

/// Works out what an object deals in, without touching it.
///
/// One signal, and it is structural on purpose: `io.TextIOBase` *is* the definition of a stream
/// whose `read` returns `str`, and `io.RawIOBase`/`io.BufferedIOBase` of one that returns `bytes`.
/// Anything the hierarchy does not cover is taken at its word, which is what keeps duck-typed
/// objects working; the capability checks are then the only requirement.
///
/// Nothing here calls a method on the object. A `read(0)` probe would name the payload exactly —
/// it is right for every standard-library object and consumes nothing — but `read` on an
/// arbitrary object is arbitrary code, with no guarantee of being free of side effects or of
/// returning at all. A handle is not a promise that the thing behind it is ready to be touched,
/// and extraction is the wrong moment to find out.
///
/// Deliberately unused for the same reason a probe is: a `mode` attribute would settle 23 of the
/// forty objects surveyed and be wrong about two, since `codecs.getreader(..)` wraps a binary file
/// and reports *its* `"rb"` while producing `str`. The `encoding` and `errors` a text stream
/// reports are a proxy for `io.TextIOBase`, which the check below already covers.
///
/// The returned string explains which rung fired, so a rejection can say why.
fn classify(obj: &Bound<'_, PyAny>) -> Result<(Payload, &'static str), Error> {
    let py = obj.py();
    // `RawIOBase` and `BufferedIOBase` are siblings under `IOBase`, not one inside the other, and
    // both halves are populated by ordinary objects: `open(p, "rb", buffering=0)` and `io.FileIO`
    // are raw, while `open(p, "rb")`, `io.BytesIO`, `gzip`, `zipfile`, sockets and subprocess
    // pipes are buffered. Nothing is both, so neither check covers the other.
    //
    // The tempting shortcut is `IOBase` and not `TextIOBase`, which would collapse the two into
    // one. It is wrong: `tempfile.SpooledTemporaryFile` derives from `IOBase` alone, so
    // `SpooledTemporaryFile(mode="w+")` would be called binary when it reads `str`.
    if obj.is_instance(text_io_base(py)?)? {
        return Ok((Payload::Text, "it is an io.TextIOBase"));
    }
    if obj.is_instance(raw_io_base(py)?)? || obj.is_instance(buffered_io_base(py)?)? {
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
        let missing = |method: &'static str| -> Result<Error, PyErr> {
            Ok(Error::MissingMethod {
                type_name: obj.get_type().name()?.to_string(),
                method,
            })
        };
        let require = |name: &Bound<'_, PyString>, method: &'static str| -> Result<(), Error> {
            if obj.hasattr(name)? {
                Ok(())
            } else {
                Err(missing(method)?)
            }
        };
        // `io.Reader` and `io.Writer`, new in 3.14. The docs call them protocols decorated with
        // `@typing.runtime_checkable` and typeshed declares them as such, but the runtime class
        // is an ABC with a `__subclasshook__`, like `collections.abc.Iterable`.
        //
        // The three ways of asking this are nested, not equivalent. `hasattr` is an ordinary
        // lookup, so `__getattr__` counts. A runtime-checkable protocol uses
        // `inspect.getattr_static` since 3.12, which sees an attribute put on the instance but
        // not one conjured by `__getattr__`. `io.Reader`'s `_check_methods` walks the class MRO
        // and sees neither, making it the narrowest of the three.
        //
        // So it is accepted *alongside* `hasattr`, never instead of it: requiring it would turn
        // away a `__getattr__` reader and an instance-assigned one, both of which work. The only
        // thing it adds is a class passed to `io.Reader.register(..)`, which declares itself
        // without having the attribute. Worth having because before 3.14 the standard library had
        // no runtime answer here at all -- `_typeshed.SupportsRead` is a stubs-only name.
        let require_protocol = |protocol: Option<&Bound<'_, PyAny>>,
                                name: &Bound<'_, PyString>,
                                method: &'static str|
         -> Result<(), Error> {
            if obj.hasattr(name)? {
                return Ok(());
            }
            match protocol {
                // Python 3.14 or newer: an explicit registration counts.
                Some(protocol) if obj.is_instance(protocol)? => Ok(()),
                _ => Err(missing(method)?),
            }
        };
        // `io.IOBase` gives every stream a `read`, a `write` and a `seek`, including the ones
        // that only raise, so `hasattr` cannot tell a reader from a writer: a file opened `"wb"`
        // has a `read` that raises `UnsupportedOperation`. The hierarchy answers that question
        // itself, and these are queries rather than operations -- the documentation for
        // `readable()` is literally "if False, read() will raise OSError".
        //
        // Only asked of `io.IOBase` instances, where the ABC defines what they mean. On anything
        // else they would be arbitrary code, so a duck-typed object is left to fail at the first
        // real call. `mode` was the other candidate and is worse on every axis: nine of the forty
        // objects surveyed are `IOBase` with no usable `mode` -- `io.BytesIO`, `io.StringIO`,
        // `io.TextIOWrapper`, `gzip.open(.., "rt")`, a text subprocess pipe -- where none lack
        // `readable()`, and it would need parsing `"r"`, `"w"`, `"a"`, `"x"` and `"+"` besides.
        let refuses = |query: &'static str| -> Result<bool, Error> {
            if !obj.is_instance(io_base(py)?)? {
                return Ok(false);
            }
            // A raise, or anything that is not a bool, is not an answer.
            Ok(obj
                .call_method0(query)
                .ok()
                .and_then(|answer| answer.extract::<bool>().ok())
                == Some(false))
        };
        let require_capability = |query: &'static str,
                                  method: &'static str,
                                  capability: &'static str|
         -> Result<(), Error> {
            if refuses(query)? {
                Err(Error::RefusesCapability {
                    type_name: obj.get_type().name()?.to_string(),
                    query,
                    method,
                    capability,
                })
            } else {
                Ok(())
            }
        };

        if READ {
            require_protocol(io_reader(py)?, intern!(py, "read"), "read")?;
            require_capability("readable", "read", "READ")?;
        }
        if WRITE {
            require_protocol(io_writer(py)?, intern!(py, "write"), "write")?;
            require_capability("writable", "write", "WRITE")?;
            // `flush` is deliberately not required. An object that has none has nothing to flush,
            // so flushing it is a no-op rather than an error, and demanding it would turn away
            // every minimal writer that implements nothing but `write`.
        }
        if SEEK {
            require(intern!(py, "seek"), "seek")?;
            require_capability("seekable", "seek", "SEEK")?;
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

/// The shared GIL-bound implementation behind [`BoundPyBinaryFile`] and [`BoundPyTextFile`].
///
/// See [`PyFile`] for why this is one type rather than two.
///
/// This is the same split `pyo3` uses for its own types: the detached form owns a `Py<PyAny>` and
/// is `Send + 'static`, and this borrows a `Python<'py>` token so the operations do not have to
/// acquire one themselves. [`std::io::Read`] and friends are implemented on both — on this one
/// directly, and on the detached form by attaching and delegating here — so a caller that already
/// holds the GIL can say so and keep control of when it is taken:
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
