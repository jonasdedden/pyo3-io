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
        let cookie: TextPosition = seek.tell().unwrap();
        let _: std::io::Result<TextPosition> = seek.seek_to(&cookie);
        let _: std::io::Result<TextPosition> = seek.rewind();
        let _: std::io::Result<TextPosition> = seek.seek_to_end();
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
    // One mention of each, so a rename or a gap in the alias table is caught here.
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

/// Detached wrappers can be handed to ordinary owned-I/O consumers without a Python lifetime.
#[test]
fn files_can_be_given_to_consumers_that_know_nothing_about_python() {
    fn takes_owned_reader<R: Read + Send + 'static>(_reader: R) {}
    fn takes_owned_writer<W: Write + Send + 'static>(_writer: W) {}
    fn takes_reader_and_seeker<R: Read + Seek + Send + 'static>(_reader: R) {}

    #[allow(dead_code)]
    fn uses(read: BinaryRead, write: BinaryWrite, seek: BinaryReadSeek) {
        // e.g. `zip::ZipArchive::new`, `csv::Reader::from_reader`, or a thread of its own.
        takes_owned_reader(read);
        takes_owned_writer(write);
        takes_reader_and_seeker(seek);
    }
}

/// The same thing at its sharpest: onto a thread that holds no GIL at all.
///
/// Each read attaches for as long as it needs and no longer. This is the case a `Bound`-based
/// design could not express.
#[test]
fn a_file_can_be_moved_to_another_thread() {
    fn spawn_with<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = reader.read_to_end(&mut sink);
        })
    }

    #[allow(dead_code)]
    fn uses(file: BinaryRead) {
        let _ = spawn_with(file);
    }
}

/// The bound form carries the same capabilities, and only the detached one leaves the thread.
#[test]
fn the_bound_form_mirrors_the_detached_one() {
    assert_read::<BoundBinaryRead<'_>>();
    assert_write::<BoundBinaryWrite<'_>>();
    assert_seek::<BoundBinaryReadSeek<'_>>();
    assert_read::<BoundBinaryReadWriteSeekFileno<'_>>();

    #[allow(dead_code)]
    fn text_api(mut read: BoundTextRead<'_>, mut seek: BoundTextReadSeek<'_>) {
        let _: std::io::Result<String> = read.read_chars(4);
        let _: std::io::Result<TextPosition> = seek.tell();
    }

    // `BoundFile` borrows the token, so it is deliberately neither `Send` nor `'static`; that is
    // the whole reason `PyFile` exists alongside it. See `tests/ui/bound_file_is_not_send.rs`.
}

/// Borrowing the Python object's descriptor is not sound; an owned duplicate supports `AsFd`.
#[test]
#[cfg(unix)]
fn files_with_fileno_can_be_given_to_descriptor_apis() {
    use std::os::fd::{AsFd, AsRawFd};

    fn takes_fd<F: AsFd>(_: F) {}
    fn takes_raw_fd<F: AsRawFd>(_: F) {}
    fn takes_owned_fd<F: AsFd + Send + 'static>(_: F) {}

    #[allow(dead_code)]
    fn uses(binary: BinaryFileno, text: TextFileno, both: BinaryReadWriteSeekFileno) {
        takes_fd(binary.try_clone_fd().unwrap());
        takes_raw_fd(text);
        takes_owned_fd(both.try_clone_fd().unwrap());
    }

    #[allow(dead_code)]
    fn bound(file: BoundBinaryFileno<'_>) {
        takes_fd(file.try_clone_fd().unwrap());
        takes_raw_fd(file);
    }
}
