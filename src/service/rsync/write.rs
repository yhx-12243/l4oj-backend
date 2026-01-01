use core::{mem::MaybeUninit, ptr, slice, str::pattern::Pattern};
use std::{fs, os::fd::AsRawFd, path::PathBuf, str::pattern::Searcher};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, simplex},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
    spawn,
};

use super::{
    file_entry::FileEntry, io::ReadVarintRsync, jumping::Jumping, multiplex::c2s_multiplex,
};
use crate::{
    libs::{error::BoxedStdError, validate::is_lean_id},
    models::user::User,
};

const SINGLE_FILE_LIMIT: usize = 0x100_0000; // 16 MB
const TOTAL_FILE_LIMIT: usize = 0x4000_0000; // 1 GB

fn check_prefix(prefix: &[u8]) -> bool {
    const B: &[u8] = b"/.lake/build/lib/lean/";
    if prefix.len() <= B.len() { B.ends_with(prefix) } else { prefix.ends_with(B) }
}

fn check_suffix(suffix: &str) -> bool {
    let b = if let Some(a) = suffix.strip_suffix(".server") { a }
    else if let Some(a) = suffix.strip_suffix(".private") { a }
    else { suffix };
    let Some(c) = b.strip_suffix(".olean") else { return false };
    if c.is_empty() { return false }
    let mut it = c.split('/');
    it.next() == Some("") && it.all(is_lean_id)
}

fn check_path(path: &[u8], uid: &str) -> usize {
    let mut ss = uid.into_searcher(unsafe { str::from_utf8_unchecked(path) });
    let Some((s, t)) = ss.next_match() else { return 0 };
    let prefix = unsafe { path.get_unchecked(..s) };
    let suffix = unsafe { path.get_unchecked(t..) };
    if check_prefix(prefix)
    && let Some(suffix) = str::from_utf8(suffix).ok()
    && check_suffix(suffix) {
        t + 1
    } else {
        0
    }
}

async fn generate_file_list<R>(mut rx: R, uid: &str) -> Result<Vec<FileEntry>, BoxedStdError>
where
    R: AsyncRead + Unpin,
{
    let mut s = Vec::<u8>::new();
    let mut mode = 0;
    let mut sha1 = [0; 20];
    let mut acc = 0;
    let mut ret = Vec::new();
    loop {
        let flag = rx.read_varint::<0>().await?;
        if flag == 0 {
            return match rx.read_varint::<0>().await? {
                0 => Ok(ret),
                e => Err(format!("invalid end byte: {e}").into()),
            };
        }
        let plen = if flag & 0x20 != 0 {
            rx.read_u8().await?.into()
        } else {
            0
        };
        if plen > s.len() {
            return Err(format!("invalid previous length {plen} > {}", s.len()).into());
        }
        unsafe { s.set_len(plen); }
        let len = if flag & 0x40 != 0 {
            rx.read_varint::<0>().await? as usize
        } else {
            rx.read_u8().await? as usize
        };
        if plen + len > 4096 {
            return Err(format!("path too long: {}", plen + len).into());
        }
        s.reserve(len);
        let slice = unsafe { slice::from_raw_parts_mut(s.as_mut_ptr().add(s.len()), len) };
        rx.read_exact(slice).await?;
        unsafe { s.set_len(plen + len); }

        let size = rx.read_varint::<2>().await?;
        if flag & 0x80 == 0 {
            rx.read_varint::<3>().await?;
        }
        if flag & 0x2000 != 0 {
            rx.read_varint::<0>().await?;
        }
        if flag & 0x2 == 0 {
            mode = rx.read_u32_le().await?.try_into()?;
        }
        let mut enabled = 0;
        match mode & libc::S_IFMT {
            | libc::S_IFREG => {
                rx.read_exact(&mut sha1).await?;
                if size as usize <= SINGLE_FILE_LIMIT && acc + size as usize <= TOTAL_FILE_LIMIT {
                    enabled = check_path(&s, uid);
                    if enabled != 0 {
                        acc += size as usize;
                    }
                }
            }
            | libc::S_IFDIR
            | libc::S_IFLNK => (), // symlink
            _ => return Err(format!("unsupported file mode: {mode:o}").into()),
        }
        let mut path = Vec::with_capacity(s.len() + 1);
        path.extend_from_slice(&s);
        unsafe { path.as_mut_ptr().add(path.len()).write(0); }
        ret.push(FileEntry { path, size: size as usize, sha1, enabled, mode });
    }
}

async fn receive<R>(mut rx: R, fl: &mut [FileEntry]) -> Result<(), BoxedStdError>
where
    R: AsyncRead + Unpin,
{
    let mut state = Jumping::default();
    let mut buf = [MaybeUninit::<u8>::uninit(); 24];
    let buf18 = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), 18) };
    let buf20 = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), 20) };
    loop {
        let idx = match state.recv(&mut rx).await {
            Ok(i) => i,
            Err(e) if e.raw_os_error() == Some(1349) => return Ok(()),
            Err(e) => return Err(e.into()),
        };
        let Some(entry) = fl.get_mut(idx as usize) else {
            return Err(format!("index {idx} out of range {}", fl.len()).into());
        };
        if entry.enabled == 0 {
            return Err(format!("I don't want file #{idx}").into());
        }
        entry.enabled = 0;
        rx.read_exact(buf18).await?;
        tracing::debug!(target: "lean4rsync-writer", "\x1b[35mfile receiving: {entry:?}\x1b[0m");
        loop {
            let size = rx.read_u32_le().await?;
            if size == 0 {
                break;
            }
            let mut w = vec![0u8; size as usize];
            rx.read_exact(&mut w).await?;
            println!("{} {:?}", w.len(), w[..w.len().min(80)].utf8_chunks().debug());
        }
        rx.read_exact(buf20).await?;
    }
}

pub async fn main(
    mut c2s: BufReader<OwnedReadHalf>,
    mut s2c: BufWriter<OwnedWriteHalf>,
    user: User,
) -> Result<(), BoxedStdError> {
    let mut l = 0u8;
    let mut buf = [MaybeUninit::<u8>::uninit(); 255];
    let _ = c2s.read(slice::from_mut(&mut l)).await?;
    let l1 = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), l.into()) };
    c2s.read_exact(l1).await?;
    tracing::debug!(target: "lean4rsync-writer", "client hash: {:?}", l1.utf8_chunks().debug());

    let (mut rx, tx) = simplex(0x4_0000);
    let mut handler = [const { None }; 256];
    handler[7] = Some(tx);
    spawn(c2s_multiplex(c2s, handler));

    let mut fl = generate_file_list(&mut rx, &user.uid).await?;
    fl.sort();
    let base_dir = PathBuf::from(format!(".internal/lean/{}", user.uid));
    if let Err(e) = fs::create_dir(&*base_dir) && e.raw_os_error() != Some(libc::EEXIST) {
        return Err(e.into());
    }
    let base_dir_fd = fs::File::open(&*base_dir)?;

    let mut state = Jumping::default();
    let mut buf = Vec::new();
    for (idx, file) in fl.iter_mut().enumerate() {
        if file.enabled == 0 {
            tracing::debug!(target: "lean4rsync-writer", "\x1b[2mfile ignored: {file:?}\x1b[0m");
            continue;
        }
        if file.agree(base_dir_fd.as_raw_fd()) {
            tracing::debug!(target: "lean4rsync-writer", "\x1b[32mfile agreed: {file:?}\x1b[0m");
            file.enabled = 0;
            continue;
        }
        tracing::debug!(target: "lean4rsync-writer", "\x1b[36mfile wanted: {file:?}\x1b[0m");
        buf.extend_from_slice(state.emit(idx as u32, &mut [0; 5]));
        buf.push(0);
        buf.push(0x80);
        buf.reserve(16);
        unsafe {
            ptr::write_bytes(buf.as_mut_ptr().add(buf.len()), 0, 16);
            buf.set_len(buf.len() + 16);
        }
        if buf.len() >= 0x80_0000 {
            s2c.write_u32_le(buf.len() as u32 | 0x0700_0000).await?;
            s2c.write_all(&buf).await?;
            buf.clear();
        }
    }
    buf.push(0);
    s2c.write_u32_le(buf.len() as u32 | 0x0700_0000).await?;
    s2c.write_all(&buf).await?;
    s2c.flush().await?;

    receive(&mut rx, &mut fl).await?;

    s2c.write_all(b"\x04\0\0\x07\0\0\0\0").await?;
    s2c.flush().await.map_err(Into::into)
}
