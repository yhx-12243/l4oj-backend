use core::{
    fmt::Arguments,
    index::Last,
    mem::{DropGuard, MaybeUninit},
    ptr, slice,
    str::pattern::Pattern,
};
use std::{
    ffi::OsStr,
    fs::{Permissions, ReadDir},
    io,
    os::{
        fd::{AsRawFd, RawFd},
        unix::fs::{DirEntryExt2, PermissionsExt},
    },
    path::PathBuf,
    str::pattern::Searcher,
    sync::Arc,
};

use hashbrown::HashSet;
use tempfile::Builder;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter, simplex},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
    spawn,
};

use super::{
    file_entry::FileEntry, io::ReadVarintRsync, jumping::Jumping, mode::Mode,
    multiplex::c2s_multiplex,
};
use crate::{
    libs::{
        db::get_connection,
        error::BoxedStdError,
        fs::{mkdir, unmap_send},
        olean, privilege,
        validate::is_lean_id,
    },
    models::user::User,
};

const SINGLE_FILE_LIMIT: usize = 0x100_0000; // 16 MB
const TOTAL_FILE_LIMIT: usize = 0x4000_0000; // 1 GB
const TOTAL_FILE_NUM: usize = 0x10_0000; // 1 M

const TOTAL_EXCEEDED: &str = "Total file size limit exceeds 1 GB. Please contact server administrator for a larger capacity.";

fn check_prefix(prefix: &[u8]) -> bool {
    const B: &[u8] = b"/.lake/build/lib/lean";
    if prefix.len() <= B.len() { B.ends_with(prefix) } else { prefix.ends_with(B) }
}

fn check_suffix(suffix: &str) -> bool {
    let b = if let Some(a) = suffix.strip_suffix(".server") { a }
    else if let Some(a) = suffix.strip_suffix(".private") { a }
    else { suffix };
    b.strip_suffix(".olean").is_some_and(|c| c.split('/').all(is_lean_id))
    // Force `Name.needsNoEscape`, prevent the outrageous case like `import «foo.olean».bar`.
}

fn check_path(path: &[u8], uid_with_slash: &str) -> usize {
    let uid_with_trailing_slash = unsafe { uid_with_slash.as_bytes().get_unchecked(1..) };
    if path.starts_with(uid_with_trailing_slash) {
        let t = uid_with_trailing_slash.len(); // t > 0.
        let suffix = unsafe { path.get_unchecked(t..) };
        if let Some(suffix) = str::from_utf8(suffix).ok()
        && check_suffix(suffix) {
            t
        } else {
            0
        }
    } else {
        let mut ss = uid_with_slash.into_searcher(unsafe { str::from_utf8_unchecked(path) });
        let Some((s, t)) = ss.next_match() else { return 0 }; // t > 0.
        let prefix = unsafe { path.get_unchecked(..s) };
        let suffix = unsafe { path.get_unchecked(t..) };
        if check_prefix(prefix)
        && let Some(suffix) = str::from_utf8(suffix).ok()
        && check_suffix(suffix) {
            t
        } else {
            0
        }
    }
}

async fn generate_file_list<R>(mut rx: R, uid_with_slash: &str, limit: usize) -> Result<(Vec<FileEntry>, usize), BoxedStdError>
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
                0 => Ok((ret, acc)),
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
                if size as usize <= SINGLE_FILE_LIMIT {
                    enabled = check_path(&s, uid_with_slash);
                    if enabled != 0 {
                        acc += size as usize;
                        if acc > limit { return Err(TOTAL_EXCEEDED.into()); }
                    }
                }
            }
            | libc::S_IFDIR
            | libc::S_IFLNK => (), // symlink
            _ => return Err(format!("unsupported file mode: {mode:o}").into()),
        }
        let mut path = Vec::with_capacity(s.len() + 1);
        path.extend_from_slice(&s);
        unsafe { path.as_mut_ptr().add(path.len()).write(0); } // make it NUL-terminated to be friendly with C.
        ret.push(FileEntry { path, size: size as usize, sha1, enabled, mode });
        if ret.len() > TOTAL_FILE_NUM {
            return Err("too many files".into());
        }
    }
}

/// Just Cthulhu.
#[allow(clippy::arc_with_non_send_sync, clippy::transmute_undefined_repr)]
fn readdir_from_rawfd(fd: RawFd) -> io::Result<ReadDir> {
    let ptr = unsafe { libc::fdopendir(fd) };
    if ptr.is_null() { return Err(io::Error::last_os_error()); }
    Ok(unsafe {
        core::mem::transmute::<[Option<Arc<[*mut libc::DIR; 4]>>; 2], ReadDir>([
            Some(Arc::new([ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), ptr])),
            None,
        ])
    })
}

fn do_delete(
    cwd: &mut PathBuf,
    exempt: &HashSet<&[u8]>,
    dir: RawFd,
    mode: Mode,
    readdir: &mut ReadDir,
) -> io::Result<(usize, bool)> {
    let mut delcnt = 0;
    let mut alived = false;
    let mut stat = MaybeUninit::<libc::stat>::uninit();
    let base_len = cwd.as_os_str().len();
    for entry in readdir {
        let entry = entry?;
        let name = entry.file_name_ref();
        let type_ = entry.file_type()?;
        cwd.push(name);
        let pname = name.as_encoded_bytes().as_ptr();
        if type_.is_dir() {
            let fd = unsafe { libc::openat(dir, pname.cast(), libc::O_DIRECTORY) };
            if fd == -1 { return Err(io::Error::last_os_error()); }
            let (sub_delcnt, sub_alived) = do_delete(cwd, exempt, fd, mode, &mut readdir_from_rawfd(fd)?)?;
            delcnt += sub_delcnt;
            if sub_alived {
                alived = true;
            } else if mode == Mode::Write {
                if unsafe { libc::unlinkat(dir, pname.cast(), libc::AT_REMOVEDIR) } != 0 { return Err(io::Error::last_os_error()); }
                delcnt += 1;
            }
        } else if exempt.contains(cwd.as_os_str().as_encoded_bytes()) {
            alived = true;
        } else {
            match mode {
                Mode::Read => #[allow(clippy::cast_sign_loss)] {
                    if unsafe { libc::fstatat(dir, pname.cast(), stat.as_mut_ptr(), 0) } != 0 { return Err(io::Error::last_os_error()); }
                    delcnt += unsafe { stat.assume_init_ref() }.st_size as usize;
                }
                Mode::Write => {
                    if unsafe { libc::unlinkat(dir, pname.cast(), 0) } != 0 { return Err(io::Error::last_os_error()); }
                    delcnt += 1;
                }
            }
        }
        cwd.as_mut_os_string().truncate(base_len);
    }

    Ok((delcnt, alived))
}

async fn receive<R>(mut rx: R, fl: &mut [FileEntry], dir: RawFd) -> Result<usize, BoxedStdError>
where
    R: AsyncRead + Unpin,
{
    let mut n = 0;
    let mut state = Jumping::default();
    let mut buf = [MaybeUninit::<u8>::uninit(); 24];
    let buf18 = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), 18) };
    let buf20 = unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr().cast(), 20) };
    let mut tb = Builder::new();
    tb.permissions(Permissions::from_mode(0o666));
    loop {
        let idx = match state.recv(&mut rx).await {
            Ok(i) => i,
            Err(e) if e.raw_os_error() == Some(1349) => return Ok(n),
            Err(e) => return Err(e.into()),
        };
        let Some(entry) = fl.get_mut(idx as usize) else {
            return Err(format!("index {idx} out of range {}", fl.len()).into());
        };
        if entry.enabled == 0 {
            return Err(format!("I don't want file #{idx}").into());
        }
        rx.read_exact(buf18).await?;
        tracing::debug!(target: "lean4rsync-writer", "\x1b[35mfile receiving: {entry:?}\x1b[0m");

        let target = unsafe { entry.path.get_unchecked_mut(entry.enabled..) };
        mkdir(target, dir)?;
        entry.enabled = 0;
        cfg_select! {
            target_os = "linux" => {
                let f = tempfile::tempfile_in(env!("LEAN4OJ_RSYNC_TMPDIR"))?;
                f.set_len(entry.size as u64)?;
            }
            _ => {
                let f = tb.tempfile_in(env!("LEAN4OJ_RSYNC_TMPDIR"))?;
                f.as_file().set_len(entry.size as u64)?;
            }
        }
        let (mut buf, g) = unsafe {
            let raw = libc::mmap(ptr::null_mut(), entry.size, libc::PROT_WRITE, libc::MAP_SHARED, f.as_raw_fd(), 0);
            (
                slice::from_raw_parts_mut(raw.cast(), entry.size),
                DropGuard::new((raw as usize, entry.size), unmap_send),
            )
        };
        loop {
            let size = rx.read_u32_le().await?;
            if size == 0 {
                break;
            }
            let Some(chunk) = buf.split_off_mut(..size as usize) else {
                return Err(format!("chunk too large: {size} / {}", buf.len()).into());
            };
            rx.read_exact(chunk).await?;
        }
        if !buf.is_empty() {
            return Err(format!("{} bytes missing", buf.len()).into());
        }
        let buf = unsafe { slice::from_raw_parts(g.0 as *const u8, entry.size) };
        if olean::lean_version(buf).is_none() {
            return Err(format!(
                "{}: not a valid olean file",
                unsafe { OsStr::from_encoded_bytes_unchecked(target) }.display(),
            ).into());
        }
        drop(g);
        cfg_select! {
            target_os = "linux" => {
                let path = tb.make_in(env!("LEAN4OJ_RSYNC_TMPDIR"), crate::libs::fs::LinuxPersist::new(f.as_raw_fd()))?.into_temp_path();
                drop(f);
            }
            _ => {
                let path = f.into_temp_path();
            }
        }
        let mut path = path.into_inner().into_path_buf().into_os_string().into_encoded_bytes();
        path.push(0);
        unsafe {
            if libc::renameat(libc::AT_FDCWD, path.as_ptr().cast(), dir, target.as_ptr().cast()) != 0 {
                return Err(io::Error::last_os_error().into());
            }
        }
        rx.read_exact(buf20).await?;
        n += 1;
    }
}

#[allow(clippy::too_many_lines)]
pub async fn main(
    mut c2s: BufReader<OwnedReadHalf>,
    mut s2c: BufWriter<OwnedWriteHalf>,
    delete: bool,
    sni: &str,
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

    if delete && rx.read_u32().await? != 0 {
        return Err("Do not specify rule explicitly (in deletion mode). Server will filter automatically.".into());
    }

    let limit = {
        let mut conn = get_connection().await?;
        if privilege::check(&user.uid, "Lean4OJ.TooManyOLeans", &mut conn).await? {
            usize::MAX
        } else {
            TOTAL_FILE_LIMIT
        }
    };
    let mut buf = String::with_capacity(env!("OLEAN_ROOT").len() + user.uid.len() + 7);
    buf.push_str(env!("OLEAN_ROOT"));
    buf.push_str("/lean/");
    buf.push_str(&user.uid);
    buf.push('/');
    let (mut fl, acc) = generate_file_list(
        &mut rx,
        unsafe { buf.get_unchecked(const { env!("OLEAN_ROOT").len() + 5 }..) },
        limit,
    ).await?;
    fl.sort();
    unsafe { *buf.as_mut_vec().get_unchecked_mut(Last) = 0; }
    if unsafe { libc::mkdir(buf.as_ptr().cast(), 0o770) } != 0 {
        let err = io::Error::last_os_error();
        if err.raw_os_error() != Some(libc::EEXIST) { return Err(err.into()); }
    }
    let base_dir_fd = unsafe { libc::open(buf.as_ptr().cast(), libc::O_DIRECTORY) };
    if base_dir_fd == -1 { return Err(io::Error::last_os_error().into()); }

    let exempt = {
        use std::slice::SliceIndex;
        fl.iter()
            .filter_map(|entry| (entry.enabled != 0).then_some(
                unsafe { &*(entry.enabled..).get_unchecked(&raw const *entry.path) }
            ))
            .collect::<HashSet<_>>()
    };

    // This fd is now managed by 🌰!
    let mut chestnut = readdir_from_rawfd(base_dir_fd)?;

    if !delete {
        let acc2 = do_delete(&mut PathBuf::new(), &exempt, base_dir_fd, Mode::Read, &mut chestnut)?.0;
        if acc + acc2 > limit { return Err(TOTAL_EXCEEDED.into()); }
    }

    let mut state = Jumping::default();
    let mut buf = Vec::new();
    let mut exp_tot = 0;
    for (idx, file) in fl.iter_mut().enumerate() {
        if file.enabled == 0 {
            tracing::debug!(target: "lean4rsync-writer", "\x1b[2mfile ignored: {file:?}\x1b[0m");
            continue;
        }
        if file.agree(base_dir_fd) {
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
        exp_tot += 1;
    }
    buf.push(0);
    s2c.write_u32_le(buf.len() as u32 | 0x0700_0000).await?;
    s2c.write_all(&buf).await?;
    s2c.flush().await?;

    let delcnt = if delete {
        do_delete(&mut PathBuf::new(), &exempt, base_dir_fd, Mode::Write, &mut chestnut)?.0
    } else {
        0
    };
    let tot = receive(&mut rx, &mut fl, base_dir_fd).await?;

    let s = {
        let dl = if delcnt != 0 {
            format_args!(", {delcnt} file(s) purged")
        } else {
            Arguments::from_str("")
        };
        format!("======== {tot}/{exp_tot} file(s) received{dl}. Go to {sni}/lean/{}/ for further check. ========\n", user.uid)
    };
    let flag = s.len() as u32 | 0x0a00_0000;
    s2c.write_u32_le(flag).await?;
    s2c.write_all(s.as_bytes()).await?;

    s2c.write_all(b"\x04\0\0\x07\0\0\0\0").await?;
    s2c.flush().await.map_err(Into::into)
}
