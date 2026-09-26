//! The table of aliases that the alias and protocol macros expand.
//!
//! Each row is `owned alias, bound alias, protocol stem: payload [capabilities]`. The names are
//! spelled out because `macro_rules!` cannot build identifiers.

macro_rules! io_types {
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

macro_rules! payload_type {
    (Binary) => {
        crate::BinaryPayload
    };
    (Text) => {
        crate::TextPayload
    };
}

// Whether a capability appears in the list.
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

pub(crate) use {has_capability, io_types, payload_type};
