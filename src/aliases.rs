//! Named capability combinations, available without introspection or generated code.

use crate::{BoundPyBinaryFile, BoundPyTextFile, PyBinaryFile, PyTextFile};

macro_rules! aliases {
    ($file:ident, $bound_file:ident; $($alias:ident, $bound:ident: $read:literal, $write:literal, $seek:literal, $fileno:literal;)*) => {
        $(
            #[doc = concat!("A [`", stringify!($file), "`] with the capabilities named by `", stringify!($alias), "`.")]
            pub type $alias = $file<$read, $write, $seek, $fileno>;
            #[doc = concat!("A [`", stringify!($alias), "`] with an attached Python token. See [`crate::BoundFile`].")]
            pub type $bound<'py> = $bound_file<'py, $read, $write, $seek, $fileno>;
        )*
    };
}

// Columns are READ, WRITE, SEEK, FILENO, matching the underlying file types.
aliases! {
    PyBinaryFile, BoundPyBinaryFile;
    BinaryFileno, BoundBinaryFileno: false, false, false, true;
    BinarySeek, BoundBinarySeek: false, false, true, false;
    BinarySeekFileno, BoundBinarySeekFileno: false, false, true, true;
    BinaryWrite, BoundBinaryWrite: false, true, false, false;
    BinaryWriteFileno, BoundBinaryWriteFileno: false, true, false, true;
    BinaryWriteSeek, BoundBinaryWriteSeek: false, true, true, false;
    BinaryWriteSeekFileno, BoundBinaryWriteSeekFileno: false, true, true, true;
    BinaryRead, BoundBinaryRead: true, false, false, false;
    BinaryReadFileno, BoundBinaryReadFileno: true, false, false, true;
    BinaryReadSeek, BoundBinaryReadSeek: true, false, true, false;
    BinaryReadSeekFileno, BoundBinaryReadSeekFileno: true, false, true, true;
    BinaryReadWrite, BoundBinaryReadWrite: true, true, false, false;
    BinaryReadWriteFileno, BoundBinaryReadWriteFileno: true, true, false, true;
    BinaryReadWriteSeek, BoundBinaryReadWriteSeek: true, true, true, false;
    BinaryReadWriteSeekFileno, BoundBinaryReadWriteSeekFileno: true, true, true, true;
}

aliases! {
    PyTextFile, BoundPyTextFile;
    TextFileno, BoundTextFileno: false, false, false, true;
    TextSeek, BoundTextSeek: false, false, true, false;
    TextSeekFileno, BoundTextSeekFileno: false, false, true, true;
    TextWrite, BoundTextWrite: false, true, false, false;
    TextWriteFileno, BoundTextWriteFileno: false, true, false, true;
    TextWriteSeek, BoundTextWriteSeek: false, true, true, false;
    TextWriteSeekFileno, BoundTextWriteSeekFileno: false, true, true, true;
    TextRead, BoundTextRead: true, false, false, false;
    TextReadFileno, BoundTextReadFileno: true, false, false, true;
    TextReadSeek, BoundTextReadSeek: true, false, true, false;
    TextReadSeekFileno, BoundTextReadSeekFileno: true, false, true, true;
    TextReadWrite, BoundTextReadWrite: true, true, false, false;
    TextReadWriteFileno, BoundTextReadWriteFileno: true, true, false, true;
    TextReadWriteSeek, BoundTextReadWriteSeek: true, true, true, false;
    TextReadWriteSeekFileno, BoundTextReadWriteSeekFileno: true, true, true, true;
}
