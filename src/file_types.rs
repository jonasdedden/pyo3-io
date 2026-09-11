//! One table for the public aliases and their optional Python protocols.

macro_rules! file_types {
    ($define:ident) => {
        $define! {
            BinaryFileno, BoundBinaryFileno: Binary [fileno];
            BinarySeek, BoundBinarySeek: Binary [seek];
            BinarySeekFileno, BoundBinarySeekFileno: Binary [seek, fileno];
            BinaryWrite, BoundBinaryWrite: Binary [write];
            BinaryWriteFileno, BoundBinaryWriteFileno: Binary [write, fileno];
            BinaryWriteSeek, BoundBinaryWriteSeek: Binary [write, seek];
            BinaryWriteSeekFileno, BoundBinaryWriteSeekFileno: Binary [write, seek, fileno];
            BinaryRead, BoundBinaryRead: Binary [read];
            BinaryReadFileno, BoundBinaryReadFileno: Binary [read, fileno];
            BinaryReadSeek, BoundBinaryReadSeek: Binary [read, seek];
            BinaryReadSeekFileno, BoundBinaryReadSeekFileno: Binary [read, seek, fileno];
            BinaryReadWrite, BoundBinaryReadWrite: Binary [read, write];
            BinaryReadWriteFileno, BoundBinaryReadWriteFileno: Binary [read, write, fileno];
            BinaryReadWriteSeek, BoundBinaryReadWriteSeek: Binary [read, write, seek];
            BinaryReadWriteSeekFileno, BoundBinaryReadWriteSeekFileno: Binary [read, write, seek, fileno];
            TextFileno, BoundTextFileno: Text [fileno];
            TextSeek, BoundTextSeek: Text [seek];
            TextSeekFileno, BoundTextSeekFileno: Text [seek, fileno];
            TextWrite, BoundTextWrite: Text [write];
            TextWriteFileno, BoundTextWriteFileno: Text [write, fileno];
            TextWriteSeek, BoundTextWriteSeek: Text [write, seek];
            TextWriteSeekFileno, BoundTextWriteSeekFileno: Text [write, seek, fileno];
            TextRead, BoundTextRead: Text [read];
            TextReadFileno, BoundTextReadFileno: Text [read, fileno];
            TextReadSeek, BoundTextReadSeek: Text [read, seek];
            TextReadSeekFileno, BoundTextReadSeekFileno: Text [read, seek, fileno];
            TextReadWrite, BoundTextReadWrite: Text [read, write];
            TextReadWriteFileno, BoundTextReadWriteFileno: Text [read, write, fileno];
            TextReadWriteSeek, BoundTextReadWriteSeek: Text [read, write, seek];
            TextReadWriteSeekFileno, BoundTextReadWriteSeekFileno: Text [read, write, seek, fileno];
        }
    };
}

// Translate the named capabilities to the four boolean parameters of PyFile/BoundFile.
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

pub(crate) use {file_types, has_capability};
