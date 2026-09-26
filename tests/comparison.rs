//! Checks what the README claims about `pyo3-filelike`'s types. If this stops compiling, the
//! README needs updating.

use pyo3_filelike::{PyBinaryFile, PyTextFile};
use std::io::{Read, Seek, Write};

fn assert_read<T: Read>() {}
fn assert_write<T: Write>() {}
fn assert_seek<T: Seek>() {}

#[test]
fn pyo3_filelike_text_files_are_still_readable_as_bytes() {
    // Re-encoded as UTF-8. Here this does not compile: see tests/ui/io_read_on_text.rs.
    assert_read::<PyTextFile>();
}

#[test]
fn pyo3_filelike_binary_files_carry_every_capability_at_once() {
    // Whatever the function needs. Here: see tests/ui/write_on_read_only.rs.
    assert_read::<PyBinaryFile>();
    assert_write::<PyBinaryFile>();
    assert_seek::<PyBinaryFile>();
}
