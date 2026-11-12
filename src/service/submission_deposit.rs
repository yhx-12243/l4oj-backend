use core::fmt::Write;
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
use hashbrown::{HashSet, hash_set::Entry};
use openssl::sha::Sha256;
use serde::Deserialize;
use tokio::sync::mpsc;

use crate::{
    libs::{db::get_connection, error::BoxedStdError, olean},
    models::submission::{
        Submission,
        SubmissionMessageAction::{self, *},
        SubmissionStatus::{self, *},
    },
};

#[derive(Deserialize)]
pub struct Jb {
    checker: String,
}

pub struct Task {
    pub sid: u32,
    pub uid: CompactString,
    pub module_name: CompactString,
    pub const_name: CompactString,
    pub imports: Vec<CompactString>,
    pub hash: [u8; 32],
    pub checker: Bytes,
}

static TX: OnceLock<mpsc::UnboundedSender<Task>> = OnceLock::new();

#[inline(always)]
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
    for i in 1..32 {
        let _ = write!(&mut s, "{:02x}", hash[i]);
    }
    s.push_str(".olean");
    s
}

fn submission_path(sid: u32) -> io::Result<String> {
    let mut s = String::with_capacity(env!("OLEAN_ROOT").len() + 25);
    s.push_str(env!("OLEAN_ROOT"));
    s.push_str("/submissions/");
    let bytes = sid.to_le_bytes();
    let _ = write!(&mut s, "{:02x}/{:02x}/{:02x}/{:02x}/", bytes[3], bytes[2], bytes[1], bytes[0]);

    let mut db = fs::DirBuilder::new();
    db.recursive(true);
    db.create(unsafe { s.get_unchecked(..s.len() - 3) })?;
    db.recursive(false);
    db.mode(0o770);
    db.create(&*s).map(|()| s)
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

fn deposit_one(
    uid: &str,
    module: &str,
    hash: &[u8; 32],
    sroot: &str,
) -> io::Result<()> {
    let src = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(uid, module);
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

    std::os::unix::fs::symlink(&*f1, &*f2)
}

fn deposit_inner(task: Task, checker: String) -> io::Result<(SubmissionStatus, SubmissionMessageAction)> {
    let sroot = submission_path(task.sid)?;

    deposit_one(&task.uid, &task.module_name, &task.hash, &sroot)?;

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

        deposit_one(&task.uid, module, &hash, &sroot)?;

        e.insert();
        imports.into_iter().filter(|import| !visited.contains(import)).collect_into(&mut queue);
    }

    deposit_main_lean(&task.uid, &task.module_name, &task.const_name, &checker, &sroot)?;

    Ok((Deposited, NoAction))
}

async fn deposit(task @ Task { sid, .. }: Task) -> Result<(), BoxedStdError> {
    let checker = match serde_json::from_slice(&task.checker) {
        Ok(Jb { checker }) => checker,
        Err(e) => return Err(e.into()),
    };
    let mut conn = get_connection().await?;
    Submission::report_status(sid, Depositing, NoAction, &mut conn).await?;
    drop(conn);

    let (status, action) = tokio::task::spawn_blocking(|| deposit_inner(task, checker)).await??;

    let mut conn = get_connection().await?;
    Submission::report_status(sid, status, action, &mut conn).await.map_err(Into::into)
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
