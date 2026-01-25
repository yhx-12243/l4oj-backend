use core::{
    ffi::CStr,
    fmt::Write,
    pin::Pin,
    task::{Context, Poll, ready},
};
use std::{
    borrow::Cow,
    collections::VecDeque,
    fs, io,
    os::unix::fs::DirBuilderExt,
    str::pattern::{Pattern, Searcher},
    sync::OnceLock,
};

use bytes::Bytes;
use compact_str::CompactString;
use futures_util::Stream;
use hashbrown::{HashSet, hash_set::Entry};
use hyper::body::Frame;
use openssl::sha::Sha256;
use parking_lot::Mutex;
use serde::Deserialize;
use smallvec::SmallVec;
use tokio::sync::{mpsc, oneshot};

#[allow(clippy::enum_glob_use)]
use crate::{
    libs::{
        db::get_connection,
        error::BoxedStdError,
        judger::task::{LeanAxiom, Task as JudgeTask},
        olean,
    },
    models::submission::{
        Submission,
        SubmissionMessageAction::{self, *},
        SubmissionStatus::{self, *},
    },
};

#[derive(Deserialize)]
pub struct Jb {
    axioms: SmallVec<[LeanAxiom; 4]>,
    checker: String,
}

pub struct Task {
    pub sid: u32,
    pub uid: CompactString,
    pub module_name: CompactString,
    pub const_name: CompactString,
    pub is_module: bool,
    pub imports: Vec<CompactString>,
    pub version: &'static str,
    pub hash: [u8; 32],
    pub checker: Bytes,
}

static TX: OnceLock<mpsc::UnboundedSender<Task>> = OnceLock::new();
static FOOD: Mutex<Vec<oneshot::Sender<JudgeTask>>> = Mutex::new(Vec::new());

#[inline(always)]
#[allow(clippy::result_large_err)]
pub fn transmit(task: Task) -> Result<(), mpsc::error::SendError<Task>> {
    {
        #[cfg(feature = "build-std")]
        unsafe { TX.get_unchecked() }
        #[cfg(not(feature = "build-std"))]
        unsafe { TX.get().unwrap_unchecked() }
    }.send(task)
}

fn cache_path(hash: &[u8; 32]) -> String {
    let mut s = String::with_capacity(env!("OLEAN_ROOT").len() + 78);
    s.push_str(env!("OLEAN_ROOT"));
    s.push_str("/cache/");
    let _ = write!(&mut s, "{:02x}", hash[0]);
    s.push('/');
    #[allow(clippy::needless_range_loop)]
    for i in 1..32 {
        let _ = write!(&mut s, "{:02x}", hash[i]);
    }
    s.push_str(".olean");
    s
}

fn submission_path(sid: u32) -> io::Result<String> {
    const ACL_EA_ACCESS: &CStr = c"system.posix_acl_access";

    let mut s = String::with_capacity(env!("OLEAN_ROOT").len() + 26);
    s.push_str(env!("OLEAN_ROOT"));
    s.push_str("/submissions/");
    let bytes = sid.to_le_bytes();
    let _ = write!(&mut s, "{:02x}/{:02x}/{:02x}/{:02x}/\0", bytes[3], bytes[2], bytes[1], bytes[0]);
    s.pop();

    let mut db = fs::DirBuilder::new();
    db.recursive(true);
    db.create(unsafe { s.get_unchecked(..s.len() - 3) })?;
    db.recursive(false);
    db.mode(0o770);
    db.create(&*s)?;
    #[cfg(target_os = "linux")]
    unsafe {
        let mut acl = *b"\x02\0\0\0\x01\0\x07\0\xff\xff\xff\xff\x02\0\x05\0\0\0\0\0\x04\0\x07\0\xff\xff\xff\xff\x10\0\x07\0\xff\xff\xff\xff \0\0\0\xff\xff\xff\xff";
        acl.as_mut_ptr().add(16).cast::<u32>().write(0x10000 + sid);
        if libc::setxattr(s.as_ptr().cast(), ACL_EA_ACCESS.as_ptr(), acl.as_ptr().cast(), 44, 0) != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(s)
}

fn deposit_main_lean(
    uid: &str,
    module_name: &str,
    const_name: &str,
    checker: &str,
    sroot: &str,
) -> io::Result<()> {
    let mut content = String::with_capacity(checker.len() + uid.len() + module_name.len() + const_name.len() + 9);
    content.push_str("import ");
    content.push_str(uid);
    content.push('.');
    content.push_str(module_name);
    content.push('\n');

    match "⍼".into_searcher(checker).next_match() {
        Some((l, r)) => {
            content.push_str(unsafe { checker.get_unchecked(..l) });
            content.push_str(const_name);
            content.push_str(unsafe { checker.get_unchecked(r..) });
        }
        None => content.push_str(checker),
    }

    fs::write(format!("{sroot}/main.lean"), content)
}

fn deposit_module_inner(
    src: &str,
    sroot: &str,
) -> io::Result<()> {
    let content = fs::read(src)?;

    let mut sha256 = Sha256::new();
    sha256.update(&content);
    let hash = sha256.finish();

    let dest = cache_path(&hash);

    match fs::hard_link(&*src, &*dest) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => { fs::copy(&*src, &*dest)?; }
        Err(e) => return Err(e),
    }

    let f0 = unsafe { src.get_unchecked(const { env!("OLEAN_ROOT").len() + 5 }..) };
    let f1 = "../".repeat(f0.bytes().filter(|&b| b == b'/').count() + 4) + unsafe { dest.get_unchecked(const { env!("OLEAN_ROOT").len() + 1 }..) };
    let f2 = sroot.to_owned() + f0;

    std::os::unix::fs::symlink(&*f1, &*f2)
}

fn deposit_one(
    uid: &str,
    module: &str,
    hash: &[u8; 32],
    sroot: &str,
    is_module: bool,
) -> io::Result<()> {
    let mut src = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(uid, module);
    let dest = cache_path(hash);

    match fs::hard_link(&*src, &*dest) {
        Ok(()) => (),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => (),
        Err(e) if e.kind() == io::ErrorKind::CrossesDevices => { fs::copy(&*src, &*dest)?; }
        Err(e) => return Err(e),
    }

    let f0 = unsafe { src.get_unchecked(const { env!("OLEAN_ROOT").len() + 5 }..) };
    let f1 = "../".repeat(f0.bytes().filter(|&b| b == b'/').count() + 4) + unsafe { dest.get_unchecked(const { env!("OLEAN_ROOT").len() + 1 }..) };
    let f2 = sroot.to_owned() + f0;
    let pos = unsafe { f2.rfind('/').unwrap_unchecked() };
    fs::create_dir_all(unsafe { f2.get_unchecked(..pos) })?;

    std::os::unix::fs::symlink(&*f1, &*f2)?;

    if is_module {
        let l = src.len();
        src.push_str(".private");
        deposit_module_inner(&src, sroot)?;

        unsafe { src.as_mut_vec().set_len(l); }
        src.push_str(".server");
        deposit_module_inner(&src, sroot)?;

        unsafe { src.as_mut_vec().set_len(l - 5); }
        src.push_str("ir");
        deposit_module_inner(&src, sroot)?;
    }
    Ok(())
}

fn deposit_inner(task: Task, checker: String) -> io::Result<(SubmissionStatus, SubmissionMessageAction)> {
    let sroot = submission_path(task.sid)?;

    deposit_one(&task.uid, &task.module_name, &task.hash, &sroot, task.is_module)?;

    let mut queue = VecDeque::<CompactString>::from(task.imports);
    let mut visited = HashSet::<CompactString>::new();
    visited.insert(task.module_name.clone());
    while let Some(module) = queue.pop_front() {
        if let Some(module_i) = module.strip_prefix(&*task.uid) && module_i.starts_with('.') {
            // nothing
        } else if olean::is_std(&module) {
            continue;
        } else {
            return Ok((InvalidImport, Replace(Cow::Owned(format!("{module}: invalid import")))));
        }
        let Entry::Vacant(e) = visited.entry(module) else { continue; };
        let module = unsafe { e.get().get_unchecked(task.uid.len() + 1..) };
        let olean_path = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(&task.uid, module);
        let display_path = unsafe { olean_path.get_unchecked(const { env!("OLEAN_ROOT").len() }..) };
        let olean = match fs::read(&*olean_path) {
            Ok(r) => r,
            Err(e) => return Ok((InvalidImport, Replace(Cow::Owned(e.to_string())))),
        };
        let Some(meta) = olean::parse_meta(&olean) else { return Ok((InvalidImport, Replace(Cow::Owned(format!("{display_path}: not a valid olean file"))))) };
        let Some(imports) = olean::parse_imports(meta) else { return Ok((InvalidImport, Replace(Cow::Owned(format!("{display_path}: cannot parse imports"))))) };

        let mut sha256 = Sha256::new();
        sha256.update(&olean);
        let hash = sha256.finish();

        deposit_one(&task.uid, module, &hash, &sroot, meta.is_module())?;

        e.insert();
        imports.into_iter().filter(|import| !visited.contains(import)).collect_into(&mut queue);
    }

    deposit_main_lean(&task.uid, &task.module_name, &task.const_name, &checker, &sroot)?;

    Ok((Deposited, NoAction))
}

#[allow(clippy::significant_drop_tightening)]
async fn deposit(task @ Task { sid, version, .. }: Task) -> Result<(), BoxedStdError> {
    let Jb { axioms, checker } = match serde_json::from_slice(&task.checker) {
        Ok(r) => r,
        Err(e) => return Err(e.into()),
    };
    let mut conn = get_connection().await?;
    Submission::report_status(sid, Depositing, NoAction, &mut conn).await?;
    drop(conn);

    let (status, action) = tokio::task::spawn_blocking(|| deposit_inner(task, checker)).await??;

    let mut conn = get_connection().await?;
    let final_status = if status == Deposited {
        let mut guard = FOOD.lock();
        #[allow(clippy::transmute_undefined_repr)]
        let axioms = unsafe { core::mem::transmute::<SmallVec<[LeanAxiom; 4]>, SmallVec<[CompactString; 4]>>(axioms) };
        let mut version4 = CompactString::with_capacity(version.len() + 1);
        version4.push('4');
        version4.push_str(version);
        loop {
            let n = guard.len();
            if n == 0 { break Deposited; }
            let idx = rand::random_range(..n);
            let sender = guard.swap_remove(idx);
            let task = JudgeTask {
                sid,
                version: version4.clone(),
                axioms: axioms.clone(),
            };
            if sender.send(task).is_ok() { break JudgerReceived; }
            tracing::info!("can't send to channel #{idx}");
            // next loop
        }
    } else {
        status
    };
    Submission::report_status(sid, final_status, action, &mut conn).await.map_err(Into::into)
}

#[repr(transparent)]
pub struct Subscription {
    inner: Option<oneshot::Receiver<JudgeTask>>,
}

impl Subscription {
    pub fn new() -> Self {
        let (tx, rx) = oneshot::channel();
        FOOD.lock().push(tx);
        tracing::info!("new judger subscription created");
        Self { inner: Some(rx) }
    }
}

impl Stream for Subscription {
    type Item = Result<Frame<Bytes>, oneshot::error::RecvError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut inner = unsafe { self.map_unchecked_mut(|x| &mut x.inner) };
        match inner.as_mut().as_pin_mut() {
            Some(rx) => {
                let task = ready!(rx.poll(cx))?;
                let ser = serde_json::to_vec(&task).unwrap();
                inner.set(None);
                Poll::Ready(Some(Ok(Frame::data(ser.into()))))
            }
            None => return Poll::Ready(None),
        }
    }
}

pub async fn main() -> io::Result<!> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    TX.get_or_init(|| tx);

    while let Some(task @ Task { sid, .. }) = rx.recv().await {
        if let Err(e) = deposit(task).await {
            tracing::warn!("error deposit submission #{sid}: {e}");
            if let Ok(mut conn) = get_connection().await {
                let _ = Submission::report_status(
                    sid, JudgementFailed,
                    Replace(Cow::Owned(e.to_string())),
                    &mut conn,
                ).await;
            }
        }
    }

    Err(io::const_error!(io::ErrorKind::BrokenPipe, "Channel was closed unexpectedly"))
}
