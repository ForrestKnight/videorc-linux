//! FIFO transport between native capture threads and the ffmpeg readers.
//!
//! Unix-only today: the Windows counterpart (named pipes) lands with the
//! Windows port (docs/windows-port-plan.md, Phase 3). Until then the
//! non-Unix stubs return `Unsupported` so callers fail with a clear
//! runtime message instead of the crate failing to compile.

use std::fs::File;
use std::io;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

#[cfg(unix)]
pub fn create(path: &Path) -> io::Result<()> {
    use std::ffi::CString;

    let c_path = CString::new(path.display().to_string()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "FIFO path contained an interior NUL byte",
        )
    })?;
    let status = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Opens the FIFO for writing without blocking on a reader, retrying every
/// `retry` until one attaches or `stop` flips. `clear_nonblock` restores
/// blocking writes once the reader is attached.
#[cfg(unix)]
pub fn open_writer(
    path: &Path,
    stop: &AtomicBool,
    retry: Duration,
    clear_nonblock: bool,
    stopped_message: &str,
) -> io::Result<File> {
    use std::ffi::CString;
    use std::os::fd::FromRawFd;
    use std::sync::atomic::Ordering;

    let c_path = CString::new(path.display().to_string())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid FIFO path"))?;

    while !stop.load(Ordering::Relaxed) {
        let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_WRONLY | libc::O_NONBLOCK) };
        if fd >= 0 {
            if clear_nonblock {
                let _ = unsafe { libc::fcntl(fd, libc::F_SETFL, 0) };
            }
            return Ok(unsafe { File::from_raw_fd(fd) });
        }

        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::ENXIO) {
            return Err(error);
        }
        std::thread::sleep(retry);
    }

    Err(io::Error::new(io::ErrorKind::Interrupted, stopped_message))
}

#[cfg(not(unix))]
pub fn create(path: &Path) -> io::Result<()> {
    let _ = path;
    Err(unsupported())
}

#[cfg(not(unix))]
pub fn open_writer(
    path: &Path,
    stop: &AtomicBool,
    retry: Duration,
    clear_nonblock: bool,
    stopped_message: &str,
) -> io::Result<File> {
    let _ = (path, stop, retry, clear_nonblock, stopped_message);
    Err(unsupported())
}

#[cfg(not(unix))]
fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "FIFO transport is not implemented on this platform yet (named pipes arrive with the Windows port)",
    )
}
