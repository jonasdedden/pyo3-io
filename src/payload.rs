//! The payload kind of a Python file-like object.

use crate::sealed::Sealed;

/// A Python object whose `read`/`write` speak `bytes`.
///
/// This is the kind [`std::io::Read`] and [`std::io::Write`] describe, so a
/// [`PyIO<BinaryPayload, ..>`](crate::PyIO) implements them directly and every byte the Rust
/// side sees is a byte the Python object produced. Nothing is decoded on the way through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinaryPayload;

/// A Python object whose `read`/`write` speak `str`.
///
/// Python counts a text stream's `read(size)` in characters, not bytes, so this deliberately does
/// *not* implement [`std::io::Read`]: there is no byte count to honour. It offers a `str`-shaped
/// API instead, and the characters cross the boundary without being encoded and re-decoded.
///
/// A Rust consumer that genuinely needs bytes should ask the caller for a binary object, or for
/// `text_object.buffer`. Going the other way, a caller holding a binary object can produce a text
/// one with `io.TextIOWrapper(binary_object, encoding=...)`, which is where the choice of encoding
/// belongs — see the crate documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextPayload;

/// The payload kind of a [`PyIO`](crate::PyIO): [`BinaryPayload`] or [`TextPayload`].
///
/// This is the kind of payload the object deals in, not Python's `mode` string: an object opened
/// `"rb"` and one opened `"r+b"` are both [`BinaryPayload`]. Sealed; there are exactly two.
pub trait Payload: Sealed {
    /// Whether this is [`TextPayload`].
    const IS_TEXT: bool;
    /// How the kind is spelled in error messages, e.g. `binary`.
    const NAME: &'static str;
    /// The other kind's spelling, e.g. `text`.
    const OTHER_NAME: &'static str;
    /// The Python type a read produces, e.g. `bytes`.
    const PAYLOAD: &'static str;
    /// The other kind's payload, e.g. `str`.
    const OTHER_PAYLOAD: &'static str;
}

impl Sealed for BinaryPayload {}
impl Payload for BinaryPayload {
    const IS_TEXT: bool = false;
    const NAME: &'static str = "binary";
    const OTHER_NAME: &'static str = "text";
    const PAYLOAD: &'static str = "bytes";
    const OTHER_PAYLOAD: &'static str = "str";
}

impl Sealed for TextPayload {}
impl Payload for TextPayload {
    const IS_TEXT: bool = true;
    const NAME: &'static str = "text";
    const OTHER_NAME: &'static str = "binary";
    const PAYLOAD: &'static str = "str";
    const OTHER_PAYLOAD: &'static str = "bytes";
}
