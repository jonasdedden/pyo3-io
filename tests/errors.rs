//! [`Error`] reaches both destinations with the right shape.
//!
//! Every failure has to be usable twice over: as an [`io::Error`] with a kind a Rust caller can
//! match on, and as the Python exception a caller on that side would expect.

use pyo3_file_typed::Error;
use std::io;

fn kind(err: Error) -> io::ErrorKind {
    io::Error::from(err).kind()
}

#[test]
fn none_from_the_object_is_would_block() {
    // Not fatal: a non-blocking stream returns None until data arrives, so a caller has to be
    // able to tell this apart from a real failure and try again.
    assert_eq!(
        kind(Error::WouldBlock {
            operation: "read",
            done: "read"
        }),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(
        kind(Error::WouldBlock {
            operation: "write",
            done: "written"
        }),
        io::ErrorKind::WouldBlock
    );
}

#[test]
fn a_misbehaving_object_is_invalid_data() {
    assert_eq!(
        kind(Error::OverlongRead { got: 10, asked: 4 }),
        io::ErrorKind::InvalidData
    );
    assert_eq!(
        kind(Error::ImpossibleWriteCount {
            reported: 10,
            available: 4,
            unit: "bytes"
        }),
        io::ErrorKind::InvalidData
    );
}

#[test]
fn a_writer_that_accepts_nothing_is_write_zero() {
    assert_eq!(
        kind(Error::WroteNothing { unit: "bytes" }),
        io::ErrorKind::WriteZero
    );
}

#[test]
fn a_bad_argument_is_invalid_input() {
    assert_eq!(
        kind(Error::OutOfRange {
            what: "seek offset"
        }),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        kind(Error::MissingMethod {
            type_name: "Thing".into(),
            method: "read"
        }),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn messages_say_what_to_do() {
    let would_block = Error::WouldBlock {
        operation: "read",
        done: "read",
    }
    .to_string();
    assert!(
        would_block.contains("a later call may succeed"),
        "{would_block}"
    );
    // The one an object gets wrong by returning None for end of stream.
    assert!(would_block.contains("empty bytes or str"), "{would_block}");

    let missing = Error::MissingMethod {
        type_name: "Thing".into(),
        method: "flush",
    }
    .to_string();
    assert_eq!(
        missing,
        "object of type Thing has no .flush() method (missing or not callable)"
    );
}

#[test]
fn it_is_a_real_std_error() {
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}
    assert_error::<Error>();
}
