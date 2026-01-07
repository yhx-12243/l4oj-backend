use core::{cmp::Ordering, fmt, mem::MaybeUninit, ptr, slice};
use std::{
    ffi::OsStr,
    fs,
    os::fd::{FromRawFd, RawFd},
    path::{Component, Components, Path},
};

use openssl::sha::Sha1;

pub struct FileEntry {
    pub(super) path: Vec<u8>,
    pub(super) size: usize,
    pub(super) sha1: [u8; 20],
    pub(super) enabled: usize,
    pub(super) mode: u16,
}

impl FileEntry {
    pub fn agree(&self, dir: RawFd) -> bool {
        let mut stat = MaybeUninit::<libc::stat>::uninit();
        let fd = unsafe { libc::openat(dir, self.path.as_ptr().add(self.enabled).cast(), libc::O_RDONLY) };
        if fd == -1 { return false; }
        let _f = unsafe { fs::File::from_raw_fd(fd) };
        if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 { return false; }
        #[allow(clippy::cast_sign_loss)]
        if unsafe { stat.assume_init_ref() }.st_size as usize != self.size { return false; }
        let mut sha1 = Sha1::new();
        let raw = unsafe { libc::mmap(ptr::null_mut(), self.size, libc::PROT_READ, libc::MAP_PRIVATE, fd, 0) };
        sha1.update(unsafe { slice::from_raw_parts(raw.cast(), self.size) });
        unsafe { libc::munmap(raw, self.size) };
        sha1.finish() == self.sha1
    }

    #[inline]
    pub fn path(&self) -> &Path {
        Path::new(unsafe { OsStr::from_encoded_bytes_unchecked(&self.path) })
    }
}

#[cfg(debug_assertions)]
impl fmt::Debug for FileEntry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct_field5_finish(
            "FileEntry",
            "path", &self.path.utf8_chunks().debug(),
            "size", &self.size,
            "sha1", &fmt::from_fn(|fmt| fmt::Display::fmt(&self.sha1.escape_ascii(), fmt)),
            "enabled", &self.enabled,
            "mode", &fmt::from_fn(|fmt| fmt::Octal::fmt(&self.mode, fmt)),
        )
    }
}

impl PartialEq for FileEntry {
    fn eq(&self, other: &Self) -> bool {
        *self.path == *other.path
    }
}

impl Eq for FileEntry {}

impl PartialOrd for FileEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(feature = "build-std")]
#[inline(always)]
const fn ε(comp: &Components) -> bool {
    comp.path.is_empty()
}

#[cfg(not(feature = "build-std"))]
#[inline(always)]
const fn ε(comp: &Components) -> bool {
    let s = ptr::from_ref(comp).cast::<usize>();
    unsafe { *s.add(1) == 0 }
}

impl Ord for FileEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut p1 = self.path().components();
        let mut p2 = other.path().components();
        loop {
            let a = p1.next();
            let b = p2.next();
            let Some(b) = b else { return if a.is_some() { Ordering::Greater } else { Ordering::Equal }; };
            let Some(a) = a else { return Ordering::Less };
            let (Component::Normal(a), Component::Normal(b)) = (a, b) else { return a.cmp(&b) };

            if a != b {
                let α = ε(&p1) && self.mode  & libc::S_IFMT != libc::S_IFDIR;
                let β = ε(&p2) && other.mode & libc::S_IFMT != libc::S_IFDIR;
                return β.cmp(&α).then_with(|| a.cmp(b));
            }
        }
    }
}
