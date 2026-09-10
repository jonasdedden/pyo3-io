//! The other half of `compile_fail`: everything that *should* exist does.
//!
//! These are compile-time assertions; reaching the test body at all means the matrix holds.

use pyo3_file_typed::*;
use std::io::{Read, Seek, Write};

fn assert_read<T: Read>() {}
fn assert_write<T: Write>() {}
fn assert_seek<T: Seek>() {}
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn binary_files_implement_the_std_io_traits_they_declare() {
    assert_read::<BinaryRead>();
    assert_read::<BinaryReadWrite>();
    assert_read::<BinaryReadSeek>();
    assert_read::<BinaryReadFileno>();
    assert_read::<BinaryReadWriteSeekFileno>();

    assert_write::<BinaryWrite>();
    assert_write::<BinaryReadWrite>();
    assert_write::<BinaryWriteSeek>();
    assert_write::<BinaryReadWriteSeekFileno>();

    assert_seek::<BinarySeek>();
    assert_seek::<BinaryReadSeek>();
    assert_seek::<BinaryWriteSeek>();
    assert_seek::<BinaryReadWriteSeekFileno>();
}

#[test]
fn every_alias_is_send_and_sync() {
    assert_send_sync::<BinaryRead>();
    assert_send_sync::<TextRead>();
    assert_send_sync::<BinaryReadWriteSeekFileno>();
    assert_send_sync::<TextReadWriteSeekFileno>();
}

#[test]
fn text_files_have_the_character_api() {
    // Only checks that the signatures resolve; nothing is called.
    #[allow(dead_code)]
    fn uses(mut read: TextRead, mut write: TextWrite, mut seek: TextReadSeek) {
        let _: std::io::Result<String> = read.read_chars(4);
        let _: std::io::Result<String> = read.read_to_string();
        let _: std::io::Result<usize> = write.write_str("x");
        let _: std::io::Result<()> = write.write_all_str("x");
        let _: std::io::Result<()> = write.flush();
        let _: std::io::Result<u64> = seek.tell();
        let _: std::io::Result<u64> = seek.seek_to(0);
        let _: std::io::Result<u64> = seek.rewind();
        let _: std::io::Result<u64> = seek.seek_to_end();
    }
}

#[test]
fn fileno_exists_exactly_where_it_was_asked_for() {
    #[allow(dead_code)]
    fn uses(binary: BinaryFileno, text: TextFileno) {
        let _: std::io::Result<i32> = binary.fileno();
        let _: std::io::Result<i32> = text.fileno();
    }
}

#[test]
fn all_thirty_aliases_exist() {
    // One mention of each, so a rename or a gap in the generator is caught here.
    fn exists<T>() {}
    exists::<BinaryRead>();
    exists::<BinaryWrite>();
    exists::<BinarySeek>();
    exists::<BinaryFileno>();
    exists::<BinaryReadWrite>();
    exists::<BinaryReadSeek>();
    exists::<BinaryReadFileno>();
    exists::<BinaryWriteSeek>();
    exists::<BinaryWriteFileno>();
    exists::<BinarySeekFileno>();
    exists::<BinaryReadWriteSeek>();
    exists::<BinaryReadWriteFileno>();
    exists::<BinaryReadSeekFileno>();
    exists::<BinaryWriteSeekFileno>();
    exists::<BinaryReadWriteSeekFileno>();
    exists::<TextRead>();
    exists::<TextWrite>();
    exists::<TextSeek>();
    exists::<TextFileno>();
    exists::<TextReadWrite>();
    exists::<TextReadSeek>();
    exists::<TextReadFileno>();
    exists::<TextWriteSeek>();
    exists::<TextWriteFileno>();
    exists::<TextSeekFileno>();
    exists::<TextReadWriteSeek>();
    exists::<TextReadWriteFileno>();
    exists::<TextReadSeekFileno>();
    exists::<TextWriteSeekFileno>();
    exists::<TextReadWriteSeekFileno>();
}

/// `flush` is the one capability that is not in the type, and this is why.
///
/// `std::io::Write::flush` is a *required* trait method, and there is no `Flush` trait in `std`
/// to implement conditionally, so `impl Write` has to provide one whatever the object has. The
/// choice is therefore between refusing objects without a `flush` and calling it only when it is
/// there; this crate does the latter, because an object with no `flush` has nothing buffered that
/// a `flush` could reach.
///
/// Making it demandable would mean a fifth capability, `io::Write` implemented only for the
/// flushable half, and an inherent write API for the other half — 46 protocols instead of 30, and
/// no `BufWriter`, `write!` or `io::copy` for a plain writer. See the README.
#[test]
fn writable_files_are_io_write_regardless_of_flush() {
    assert_write::<BinaryWrite>();
    assert_write::<BinaryReadWrite>();
}
