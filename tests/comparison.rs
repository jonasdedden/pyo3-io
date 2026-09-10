//! Mechanically checks what the README claims about `pyo3-filelike`.
//!
//! `pyo3-filelike` already makes the payload kind static, with a `PyBinaryFile` and a
//! `PyTextFile`, and its text reads work at every length because it buffers internally. These are
//! the two places where its types still allow something this crate's do not, asserted here so the
//! comparison cannot go stale: if either stops compiling, the README needs updating.

use pyo3_filelike::{PyBinaryFile, PyTextFile};
use std::io::{Read, Seek, Write};

fn assert_read<T: Read>() {}
fn assert_write<T: Write>() {}
fn assert_seek<T: Seek>() {}

#[test]
fn pyo3_filelike_text_files_are_still_readable_as_bytes() {
    // `PyTextFile: Read` means a text object still reaches a bytes-oriented consumer, re-encoded
    // as UTF-8. That is what makes `test_vs_pyo3_filelike.py` able to observe a latin-1 file
    // arriving as different bytes than it holds.
    assert_read::<PyTextFile>();

    // The equivalent does not compile here: see tests/ui/io_read_on_text.rs and
    // tests/ui/gzip_cannot_take_text.rs.
}

#[test]
fn pyo3_filelike_binary_files_carry_every_capability_at_once() {
    // One type with all three, whatever the function actually needs, so writing to a handle the
    // caller opened for reading is a runtime error rather than a compile error.
    assert_read::<PyBinaryFile>();
    assert_write::<PyBinaryFile>();
    assert_seek::<PyBinaryFile>();

    // Here the capabilities are separate: `BinaryRead` is `Read` and nothing else. See
    // tests/ui/write_on_read_only.rs, tests/ui/seek_without_capability.rs.
}

// `PyTextFile` correctly has no `Seek` at all, for the same reason this crate does not implement
// `std::io::Seek` for text: Python text streams seek by character. That is a negative, so there is
// nothing to assert here — `assert_seek::<PyTextFile>()` would simply not compile, in either
// crate. Where they differ is that this crate still offers the cookie API Python does support:
// `tell` and `seek_to`, covered by tests/api_surface.rs.
