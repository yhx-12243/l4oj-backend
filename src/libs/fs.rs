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
            println!("create -> {ret}");
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
