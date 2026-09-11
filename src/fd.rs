//! Unix descriptor access. A Python reference keeps the object alive, not its descriptor open.
//!
//! There is deliberately no `AsFd`: Python can close the descriptor during a Rust borrow.
//! Use fallible `fileno()` for a number, or `try_clone_fd()` for an independently owned duplicate.

use crate::{BoundFile, Mode, PyFile};
use pyo3::Python;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

fn expect_fd(fd: io::Result<i32>) -> RawFd {
    fd.unwrap_or_else(|err| {
        panic!(
            "fileno() failed: {err}. AsRawFd cannot return an error; \
             use PyFile::fileno for the fallible form"
        )
    })
}

fn duplicate(fd: RawFd) -> io::Result<OwnedFd> {
    // SAFETY: fcntl takes integer arguments here. Invalid/closed descriptors produce an OS
    // error; no BorrowedFd (and hence no promise about the foreign descriptor's lifetime)
    // is manufactured. The kernel atomically sets close-on-exec on the new descriptor.
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful F_DUPFD_CLOEXEC returned a new open descriptor owned by this call.
    Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool> PyFile<M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// Duplicates `fileno()` into an independently owned, close-on-exec descriptor.
    ///
    /// Closing the Python object afterwards does not close the duplicate. Both descriptors
    /// share the OS file offset, but operations on the duplicate bypass Python's buffers.
    /// Flush/synchronize buffered I/O before mixing the two interfaces.
    ///
    /// Coordinate concurrent closing or replacement of the original descriptor if the
    /// identity of the duplicated resource matters. Holding a Python reference alone does
    /// not prevent another owner from closing and reusing its descriptor number.
    pub fn try_clone_fd(&self) -> io::Result<OwnedFd> {
        Python::attach(|py| self.bind(py).try_clone_fd())
    }
}

impl<M, const READ: bool, const WRITE: bool, const SEEK: bool>
    BoundFile<'_, M, READ, WRITE, SEEK, true>
where
    M: Mode,
{
    /// Duplicates the descriptor. See [`PyFile::try_clone_fd`] for ownership and buffering.
    pub fn try_clone_fd(&self) -> io::Result<OwnedFd> {
        duplicate(self.fileno()?)
    }
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
