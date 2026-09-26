//! The payload kind of a Python file-like object.

use crate::sealed::Sealed;

/// A Python object whose `read`/`write` speak `bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BinaryPayload;

/// A Python object whose `read`/`write` speak `str`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextPayload;

/// The payload kind of a [`PyIO`](crate::PyIO): [`BinaryPayload`] or [`TextPayload`]. Sealed.
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
