#[cfg(target_os = "linux")]
use std::path::Path;
use std::{io, os::fd::RawFd};

#[cfg(false)]
mod legacy {

use serde_json::Value;
use tower_sessions_core::Session;

use crate::models::user::User;

use super::super::{db::get_connection, session::GlobalStore, validate::is_lean_id_rest};

fn is_his_module(path: &str, uid: &str) -> bool {
    if let Some(next) = path.strip_prefix('/')
    && let Some(next) = next.strip_prefix(uid) {
        match next.chars().next() {
            Some(ch) => !is_lean_id_rest(ch),
            None => true,
        }
    } else {
        false
    }
}

fn is_public_module(path: &str) -> bool {
    is_his_module(path, "Lean4OJ")
}

pub async fn is_read_forbidden(path: &str, session: Option<Session<GlobalStore>>) -> bool {
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    if !(path.ends_with(".olean") || path.ends_with(".olean.server") || path.ends_with(".olean.private")) {
        return false;
    }

    if is_public_module(path) {
        return false;
    }

    let Some(session) = session else { return true };
    let Ok(Some(Value::String(uid))) = session.get_value("uid").await else { return true };
    if is_his_module(path, &uid) {
        return false;
    }

    let Ok(mut conn) = get_connection().await else { return true };
    let Ok(Some(user)) = User::by_uid(&uid, &mut conn).await else { return true };
    // check whether user has permission to view that module.

    true
}

}

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
        unsafe {
            fd.cast_unsigned()._fmt(core::slice::from_raw_parts_mut(buf.as_mut_ptr().add(14).cast(), 10));
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
