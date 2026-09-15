//! The other half of `compile_fail`: everything that *should* exist does.
//!
//! These are compile-time assertions; reaching the test body at all means the matrix holds.

use pyo3_typed_io::*;
use std::io::{Read, Seek, Write};

fn assert_read<T: Read>() {}
fn assert_write<T: Write>() {}
fn assert_seek<T: Seek>() {}
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn binary_files_implement_the_std_io_traits_they_declare() {
    assert_read::<PyBinaryRead>();
    assert_read::<PyBinaryReadWrite>();
    assert_read::<PyBinaryReadSeek>();
    assert_read::<PyBinaryReadFileno>();
    assert_read::<PyBinaryReadWriteSeekFileno>();

    assert_write::<PyBinaryWrite>();
    assert_write::<PyBinaryReadWrite>();
    assert_write::<PyBinaryWriteSeek>();
    assert_write::<PyBinaryReadWriteSeekFileno>();

    assert_seek::<PyBinarySeek>();
    assert_seek::<PyBinaryReadSeek>();
    assert_seek::<PyBinaryWriteSeek>();
    assert_seek::<PyBinaryReadWriteSeekFileno>();
}

#[test]
fn every_alias_is_send_and_sync() {
    assert_send_sync::<PyBinaryRead>();
    assert_send_sync::<PyTextRead>();
    assert_send_sync::<PyBinaryReadWriteSeekFileno>();
    assert_send_sync::<PyTextReadWriteSeekFileno>();
}

#[test]
fn text_files_have_the_character_api() {
    // Only checks that the signatures resolve; nothing is called.
    #[allow(dead_code)]
    fn uses(mut read: PyTextRead, mut write: PyTextWrite, mut seek: PyTextReadSeek) {
        let _: std::io::Result<String> = read.read_chars(4);
        let _: std::io::Result<String> = read.read_to_string();
        let _: std::io::Result<usize> = write.write_str("x");
        let _: std::io::Result<()> = write.write_all_str("x");
        let _: std::io::Result<()> = write.flush();
        let cookie: PyTextPosition = seek.tell().unwrap();
        let _: std::io::Result<PyTextPosition> = seek.seek_to(&cookie);
        let _: std::io::Result<PyTextPosition> = seek.rewind();
        let _: std::io::Result<PyTextPosition> = seek.seek_to_end();
    }
}

#[test]
fn fileno_exists_exactly_where_it_was_asked_for() {
    #[allow(dead_code)]
    fn uses(binary: PyBinaryFileno, text: PyTextFileno) {
        let _: std::io::Result<i32> = binary.fileno();
        let _: std::io::Result<i32> = text.fileno();
    }
}

#[test]
fn all_thirty_aliases_exist() {
    // One mention of each, so a rename or a gap in the alias table is caught here.
    fn exists<T>() {}
    exists::<PyBinaryRead>();
    exists::<PyBinaryWrite>();
    exists::<PyBinarySeek>();
    exists::<PyBinaryFileno>();
    exists::<PyBinaryReadWrite>();
    exists::<PyBinaryReadSeek>();
    exists::<PyBinaryReadFileno>();
    exists::<PyBinaryWriteSeek>();
    exists::<PyBinaryWriteFileno>();
    exists::<PyBinarySeekFileno>();
    exists::<PyBinaryReadWriteSeek>();
    exists::<PyBinaryReadWriteFileno>();
    exists::<PyBinaryReadSeekFileno>();
    exists::<PyBinaryWriteSeekFileno>();
    exists::<PyBinaryReadWriteSeekFileno>();
    exists::<PyTextRead>();
    exists::<PyTextWrite>();
    exists::<PyTextSeek>();
    exists::<PyTextFileno>();
    exists::<PyTextReadWrite>();
    exists::<PyTextReadSeek>();
    exists::<PyTextReadFileno>();
    exists::<PyTextWriteSeek>();
    exists::<PyTextWriteFileno>();
    exists::<PyTextSeekFileno>();
    exists::<PyTextReadWriteSeek>();
    exists::<PyTextReadWriteFileno>();
    exists::<PyTextReadSeekFileno>();
    exists::<PyTextWriteSeekFileno>();
    exists::<PyTextReadWriteSeekFileno>();
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
    assert_write::<PyBinaryWrite>();
    assert_write::<PyBinaryReadWrite>();
}

/// Detached wrappers can be handed to ordinary owned-I/O consumers without a Python lifetime.
#[test]
fn files_can_be_given_to_consumers_that_know_nothing_about_python() {
    fn takes_owned_reader<R: Read + Send + 'static>(_reader: R) {}
    fn takes_owned_writer<W: Write + Send + 'static>(_writer: W) {}
    fn takes_reader_and_seeker<R: Read + Seek + Send + 'static>(_reader: R) {}

    #[allow(dead_code)]
    fn uses(read: PyBinaryRead, write: PyBinaryWrite, seek: PyBinaryReadSeek) {
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
    fn uses(file: PyBinaryRead) {
        let _ = spawn_with(file);
    }
}

/// The bound form carries the same capabilities, and only the detached one leaves the thread.
#[test]
fn the_bound_form_mirrors_the_detached_one() {
    assert_read::<BoundPyBinaryRead<'_>>();
    assert_write::<BoundPyBinaryWrite<'_>>();
    assert_seek::<BoundPyBinaryReadSeek<'_>>();
    assert_read::<BoundPyBinaryReadWriteSeekFileno<'_>>();

    #[allow(dead_code)]
    fn text_api(mut read: BoundPyTextRead<'_>, mut seek: BoundPyTextReadSeek<'_>) {
        let _: std::io::Result<String> = read.read_chars(4);
        let _: std::io::Result<PyTextPosition> = seek.tell();
    }

    // `BoundPyIO` borrows the token, so it is deliberately neither `Send` nor `'static`; that is
    // the whole reason `PyIO` exists alongside it. See `tests/ui/bound_file_is_not_send.rs`.
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
    fn uses(binary: PyBinaryFileno, text: PyTextFileno, both: PyBinaryReadWriteSeekFileno) {
        takes_fd(binary.try_clone_fd().unwrap());
        takes_raw_fd(text);
        takes_owned_fd(both.try_clone_fd().unwrap());
    }

    #[allow(dead_code)]
    fn bound(file: BoundPyBinaryFileno<'_>) {
        takes_fd(file.try_clone_fd().unwrap());
        takes_raw_fd(file);
    }
}
