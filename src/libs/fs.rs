#[cfg(target_os = "linux")]
use std::path::Path;
use std::{io, os::fd::RawFd};

pub fn mkdir(path: &mut [u8], dir: RawFd) -> io::Result<()> {
    let p = path.as_ptr().cast();
    for byte in path {
        if *byte == b'/' {
            *byte = 0;
            let ret = unsafe { libc::mkdirat(dir, p, 0o777) };
            *byte = b'/';
            if ret != 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() != Some(libc::EEXIST) {
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

pub fn unmap_send((ptr, size): (usize, usize)) {
    unsafe { libc::munmap(ptr as _, size); }
}

#[cfg(target_os = "linux")]
pub struct LinuxPersist {
    buf: [core::ascii::Char; 24],
}

#[cfg(target_os = "linux")]
impl LinuxPersist {
    pub fn new(fd: RawFd) -> Self {
        let mut buf = const { *b"/proc/self/fd/\0\0\0\0\0\0\0\0\0\0".as_ascii().unwrap() };
        let mut buf_ = core::fmt::NumBuffer::new();
        let fds = fd.cast_unsigned().format_into(&mut buf_);
        unsafe {
            core::ptr::copy_nonoverlapping(fds.as_ptr(), buf.as_mut_ptr().add(14).cast(), fds.len());
        }
        Self { buf }
    }
}

#[cfg(target_os = "linux")]
impl FnOnce<(&Path,)> for LinuxPersist {
    type Output = io::Result<()>;

    extern "rust-call" fn call_once(mut self, args: (&Path,)) -> Self::Output {
        self.call_mut(args)
    }
}

#[cfg(target_os = "linux")]
impl FnMut<(&Path,)> for LinuxPersist {
    extern "rust-call" fn call_mut(&mut self, (path,): (&Path,)) -> Self::Output {
        match unsafe {
            libc::linkat(libc::AT_FDCWD, self.buf.as_ptr().cast(), libc::AT_FDCWD, path.as_os_str().as_encoded_bytes().as_ptr().cast(), libc::AT_SYMLINK_FOLLOW)
        } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }
}
