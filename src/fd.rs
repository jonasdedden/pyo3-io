//! Unix descriptor access.
//!
//! There is deliberately no `AsFd`: holding the Python object does not keep its descriptor open.

use crate::{BoundPyIO, Payload, PyIO};
use pyo3::Python;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

fn expect_fd(fd: io::Result<i32>) -> RawFd {
    fd.unwrap_or_else(|err| {
        panic!(
            "fileno() failed: {err}. AsRawFd cannot return an error; \
             use PyIO::fileno for the fallible form"
        )
    })
}

fn duplicate(fd: RawFd) -> io::Result<OwnedFd> {
    // SAFETY: fcntl only takes integers here; an invalid descriptor is an OS error.
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: successful F_DUPFD_CLOEXEC returned a new open descriptor owned by this call.
    Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool> PyIO<P, READ, WRITE, SEEK, true>
where
    P: Payload,
{
    /// Duplicates `fileno()` into an owned, close-on-exec descriptor that outlives the Python
    /// object.
    ///
    /// The duplicate shares the file offset but bypasses Python's buffers, so flush before mixing
    /// the two.
    pub fn try_clone_fd(&self) -> io::Result<OwnedFd> {
        Python::attach(|py| self.bind(py).try_clone_fd())
    }
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool>
    BoundPyIO<'_, P, READ, WRITE, SEEK, true>
where
    P: Payload,
{
    /// Duplicates the descriptor. See [`PyIO::try_clone_fd`].
    pub fn try_clone_fd(&self) -> io::Result<OwnedFd> {
        duplicate(self.fileno()?)
    }
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool> AsRawFd
    for PyIO<P, READ, WRITE, SEEK, true>
where
    P: Payload,
{
    /// # Panics
    ///
    /// If `fileno()` raises. Use [`fileno`](PyIO::fileno) to handle that.
    fn as_raw_fd(&self) -> RawFd {
        expect_fd(self.fileno())
    }
}

impl<P, const READ: bool, const WRITE: bool, const SEEK: bool> AsRawFd
    for BoundPyIO<'_, P, READ, WRITE, SEEK, true>
where
    P: Payload,
{
    /// # Panics
    ///
    /// If `fileno()` raises. Use [`fileno`](BoundPyIO::fileno) to handle that.
    fn as_raw_fd(&self) -> RawFd {
        expect_fd(self.fileno())
    }
}
