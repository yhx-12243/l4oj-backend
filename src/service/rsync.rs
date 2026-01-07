use core::{ascii::Char, mem};

use base64::{Engine, prelude::BASE64_STANDARD};
use openssl::sha::Sha256;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter},
    net::{
        UnixListener, UnixStream,
        unix::{OwnedReadHalf, OwnedWriteHalf},
    },
};

use crate::{
    libs::{
        constants::PASSWORD_LENGTH,
        db::get_connection,
        error::{BoxedStdError, serialize_err},
        util::gen_random_ascii,
    },
    models::user::User,
};
use io::ReadPossibleLine;
use mode::Mode;

mod file_entry;
mod io;
mod jumping;
mod mode;
mod multiplex;
mod read;
mod write;

fn protocol_version(s: &str) -> Option<u32> {
    let suffix = s.strip_prefix("@RSYNCD: ")?.as_bytes();
    let n = suffix.iter().position(|&b| !b.is_ascii_digit()).unwrap_or(suffix.len());
    u32::from_ascii(unsafe { suffix.get_unchecked(..n) }).ok()
}

#[inline]
fn check_password(password: &[u8; PASSWORD_LENGTH], salt: &[Char; 16], response: Option<&str>) -> bool {
    let found = if let Some(a) = response && let Some(b) = a.as_bytes().as_array::<PASSWORD_LENGTH>() {
        b
    } else {
        return false;
    };
    let mut sha256 = Sha256::new();
    sha256.update(password);
    sha256.update(salt.as_bytes());
    let hash = sha256.finish();
    let mut b64hash = [0u8; PASSWORD_LENGTH];
    BASE64_STANDARD.internal_encode(&hash, &mut b64hash);
    b64hash == *found
}

async fn main_inner(
    c2s: OwnedReadHalf,
    s2c: OwnedWriteHalf,
    #[cfg_attr(debug_assertions, allow(unused_variables))]
    salt: [Char; 16],
) -> Result<(), BoxedStdError> {
    let mut c2s = BufReader::new(c2s);
    let s2c = BufWriter::new(s2c);

    let mut s = String::new();
    (&mut c2s).take(1024).read_line(&mut s).await?;
    let Some(30..) = protocol_version(&s) else {
        return Err(format!("invalid first line: {s}").into());
    };
    s.clear();
    (&mut c2s).take(1024).read_line(&mut s).await?;
    if s.pop() != Some('\n') {
        return Err(format!("invalid second line (module): {s}").into());
    }

    let uid = mem::take(&mut s);
    (&mut c2s).take(1024).read_line(&mut s).await?;

    let mut mode = Mode::Write;
    loop {
        let s = unsafe { &*c2s.read_possible_line::<0, 10>().await? };
        if s == b"--sender" {
            mode = Mode::Read;
        }
        if s.is_empty() {
            break;
        }
    }

    match mode {
        Mode::Read => read::main(c2s, s2c),
        Mode::Write => {
            let user = {
                let mut conn = get_connection().await?;
                match User::by_uid(&uid, &mut conn).await? {
                    Some(u) => u,
                    None => return Err(format!("unknown user: {uid}").into()),
                }
            };

            #[cfg(debug_assertions)]
            return write::main(c2s, s2c, user).await;

            #[cfg(not(debug_assertions))]
            if check_password(&user.password, &salt, s.split_ascii_whitespace().nth(1)) {
                write::main(c2s, s2c, user).await
            } else {
                return Err(format!("authentication failed for user: {uid}").into());
            }
        }
    }
}

async fn handle(mut socket: UnixStream) {
    let buf = gen_random_ascii::<16>();
    let _ = socket.write_all(b"@RSYNCD: 32.0 sha256\n@RSYNCD: AUTHREQD ").await;
    let _ = socket.write_all(buf.as_bytes()).await;
    let _ = socket.write_all(b"\n@RSYNCD: OK\n\x81\xfe\x04sha1\0\0\0\0").await;
    let (c2s, s2c, mut socket) = socket.tri_split();
    let res = main_inner(c2s, s2c, buf).await;
    let socket = unsafe { std::sync::Arc::get_mut_unchecked(&mut socket) };
    if let Err(e) = res {
        tracing::info!(target: "lean4rsync", "failed to handle rsync client: {e:?}");

        let mut e = serialize_err(&*e);
        e.truncate(0x00ff_fffe);
        e.push(b'\n');
        let flag = e.len() as u32 | 0x0a00_0000;
        let _ = socket.write_u32_le(flag).await;
        let _ = socket.write_all(&e).await;
    }
    let _ = socket.shutdown().await;
}

pub async fn main() -> std::io::Result<!> {
    const SOCK: &str = "lean4rsync.sock";

    if let Err(err) = std::fs::remove_file(SOCK) && err.kind() != std::io::ErrorKind::NotFound {
        return Err(err);
    }

    let listener = UnixListener::bind(SOCK)?;

    loop {
        let socket = match listener.accept().await {
            Ok((socket, _)) => socket,
            Err(e) => {
                tracing::warn!(target: "lean4rsync", "server accept error: {e:?}");
                continue;
            }
        };

        tokio::spawn(handle(socket));
    }
}
