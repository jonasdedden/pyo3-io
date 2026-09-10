//! File-descriptor access, for the capabilities that asked for it.
//!
//! [`AsRawFd`] and [`AsFd`] cannot report failure, and `fileno()` raising is entirely normal in
//! Python — `io.BytesIO` has one that always does. So these panic where
//! [`fileno`](crate::PyFile::fileno) returns an error, and the fallible form is the one to reach
//! for unless a trait bound forces otherwise. They exist because the rest of the ecosystem is
//! written against the traits: `rustix`, `nix`, `memmap2` and `std` all take `impl AsFd`.

use crate::{BoundFile, Mode, PyFile};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, RawFd};

fn expect_fd(fd: std::io::Result<i32>) -> RawFd {
    fd.unwrap_or_else(|err| {
        panic!(
            "fileno() failed: {err}. AsRawFd and AsFd cannot return an error; \
             use PyFile::fileno for the fallible form"
        )
    })
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool> AsRawFd
    for PyFile<M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// # Panics
    ///
    /// If `fileno()` raises, which Python objects without a descriptor do. Use
    /// [`fileno`](PyFile::fileno) to handle that.
    fn as_raw_fd(&self) -> RawFd {
        expect_fd(self.fileno())
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool> AsFd
    for PyFile<M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// # Panics
    ///
    /// As [`as_raw_fd`](AsRawFd::as_raw_fd).
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: the descriptor belongs to the Python object, which this value holds a reference
        // to, so it cannot be collected while the borrow lasts. It could still be closed out from
        // under us by Python calling `close()`, which is the same exposure every wrapper of a
        // foreign descriptor has and why `fileno` is the recommended form.
        unsafe { BorrowedFd::borrow_raw(expect_fd(self.fileno())) }
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool> AsRawFd
    for BoundFile<'_, M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// # Panics
    ///
    /// As [`AsRawFd::as_raw_fd`] for [`PyFile`].
    fn as_raw_fd(&self) -> RawFd {
        expect_fd(self.fileno())
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool> AsFd
    for BoundFile<'_, M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// # Panics
    ///
    /// As [`AsRawFd::as_raw_fd`] for [`PyFile`].
    fn as_fd(&self) -> BorrowedFd<'_> {
        // SAFETY: as the `PyFile` implementation above.
        unsafe { BorrowedFd::borrow_raw(expect_fd(self.fileno())) }
    }
}
