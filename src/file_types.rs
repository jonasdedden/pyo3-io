//! One table for the public aliases and their optional Python protocols.
//!
//! Each row is `owned, bound, protocol: payload [capabilities]`:
//!
//! - `owned` and `bound` are the Rust type aliases, spelled `Py…` and `BoundPy…`.
//! - `protocol` is the stem of the Python `typing.Protocol` the stub generator emits,
//!   as `Supports<protocol>`. It is deliberately **not** derived from the Rust name:
//!   Python types carry no `Py` prefix, so the two must be free to differ.
//! - `payload` is a tag, `Binary` or `Text`, that the consuming macros match on and spell
//!   into documentation; `payload_type!` maps it to [`BinaryPayload`](crate::BinaryPayload)
//!   or [`TextPayload`](crate::TextPayload).

macro_rules! file_types {
    ($define:ident) => {
        $define! {
            PyBinaryFileno, BoundPyBinaryFileno, BinaryFileno: Binary [fileno];
            PyBinarySeek, BoundPyBinarySeek, BinarySeek: Binary [seek];
            PyBinarySeekFileno, BoundPyBinarySeekFileno, BinarySeekFileno: Binary [seek, fileno];
            PyBinaryWrite, BoundPyBinaryWrite, BinaryWrite: Binary [write];
            PyBinaryWriteFileno, BoundPyBinaryWriteFileno, BinaryWriteFileno: Binary [write, fileno];
            PyBinaryWriteSeek, BoundPyBinaryWriteSeek, BinaryWriteSeek: Binary [write, seek];
            PyBinaryWriteSeekFileno, BoundPyBinaryWriteSeekFileno, BinaryWriteSeekFileno: Binary [write, seek, fileno];
            PyBinaryRead, BoundPyBinaryRead, BinaryRead: Binary [read];
            PyBinaryReadFileno, BoundPyBinaryReadFileno, BinaryReadFileno: Binary [read, fileno];
            PyBinaryReadSeek, BoundPyBinaryReadSeek, BinaryReadSeek: Binary [read, seek];
            PyBinaryReadSeekFileno, BoundPyBinaryReadSeekFileno, BinaryReadSeekFileno: Binary [read, seek, fileno];
            PyBinaryReadWrite, BoundPyBinaryReadWrite, BinaryReadWrite: Binary [read, write];
            PyBinaryReadWriteFileno, BoundPyBinaryReadWriteFileno, BinaryReadWriteFileno: Binary [read, write, fileno];
            PyBinaryReadWriteSeek, BoundPyBinaryReadWriteSeek, BinaryReadWriteSeek: Binary [read, write, seek];
            PyBinaryReadWriteSeekFileno, BoundPyBinaryReadWriteSeekFileno, BinaryReadWriteSeekFileno: Binary [read, write, seek, fileno];
            PyTextFileno, BoundPyTextFileno, TextFileno: Text [fileno];
            PyTextSeek, BoundPyTextSeek, TextSeek: Text [seek];
            PyTextSeekFileno, BoundPyTextSeekFileno, TextSeekFileno: Text [seek, fileno];
            PyTextWrite, BoundPyTextWrite, TextWrite: Text [write];
            PyTextWriteFileno, BoundPyTextWriteFileno, TextWriteFileno: Text [write, fileno];
            PyTextWriteSeek, BoundPyTextWriteSeek, TextWriteSeek: Text [write, seek];
            PyTextWriteSeekFileno, BoundPyTextWriteSeekFileno, TextWriteSeekFileno: Text [write, seek, fileno];
            PyTextRead, BoundPyTextRead, TextRead: Text [read];
            PyTextReadFileno, BoundPyTextReadFileno, TextReadFileno: Text [read, fileno];
            PyTextReadSeek, BoundPyTextReadSeek, TextReadSeek: Text [read, seek];
            PyTextReadSeekFileno, BoundPyTextReadSeekFileno, TextReadSeekFileno: Text [read, seek, fileno];
            PyTextReadWrite, BoundPyTextReadWrite, TextReadWrite: Text [read, write];
            PyTextReadWriteFileno, BoundPyTextReadWriteFileno, TextReadWriteFileno: Text [read, write, fileno];
            PyTextReadWriteSeek, BoundPyTextReadWriteSeek, TextReadWriteSeek: Text [read, write, seek];
            PyTextReadWriteSeekFileno, BoundPyTextReadWriteSeekFileno, TextReadWriteSeekFileno: Text [read, write, seek, fileno];
        }
    };
}

// The payload tag in the table above is matched on; this maps it to the marker type.
macro_rules! payload_type {
    (Binary) => {
        crate::BinaryPayload
    };
    (Text) => {
        crate::TextPayload
    };
}

// Translate the named capabilities to the four boolean parameters of PyFile/BoundPyFile.
macro_rules! has_capability {
    (read; read $(, $rest:ident)*) => { true };
    (write; write $(, $rest:ident)*) => { true };
    (seek; seek $(, $rest:ident)*) => { true };
    (fileno; fileno $(, $rest:ident)*) => { true };
    ($wanted:ident; $first:ident $(, $rest:ident)*) => {
        has_capability!($wanted; $($rest),*)
    };
    ($wanted:ident;) => { false };
}

pub(crate) use {file_types, has_capability, payload_type};
