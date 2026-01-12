use core::fmt::Write;
use std::{
    fs, io, os::unix::fs::DirBuilderExt, str::pattern::{Pattern, Searcher}, sync::OnceLock
};

use compact_str::CompactString;
use tokio::sync::mpsc;

use crate::{
    libs::{db::get_connection, error::BoxedStdError, olean},
    models::submission::{Submission, SubmissionStatus},
};

pub struct Task {
    pub sid: u32,
    pub uid: CompactString,
    pub module_name: CompactString,
    pub const_name: CompactString,
    pub imports: Vec<CompactString>,
    pub hash: [u8; 32],
    pub checker: serde_json::Result<String>,
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

pub fn cache_path(hash: &[u8; 32]) -> String {
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

pub fn submission_path(sid: u32) -> io::Result<String> {
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

pub fn deposit_one(
    uid: &str,
    module: &str,
    content: &[u8],
    hash: &[u8; 32],
    sroot: &str,
) -> io::Result<()> {
    let src = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(uid, module);
    let dest = cache_path(hash);
    println!("deposit {src} -> {dest}");

    // todo
    Ok(())
}

fn deposit_inner(task: Task) -> Result<(), BoxedStdError> {
    let checker = task.checker?;
    let sroot = submission_path(task.sid)?;

    let mut content = String::with_capacity(checker.len() + task.uid.len() + task.module_name.len() + task.const_name.len() + 9);
    content.push_str("import ");
    content.push_str(&task.uid);
    content.push('.');
    content.push_str(&task.module_name);
    content.push('\n');

    match "⍼".into_searcher(&checker).next_match() {
        Some((l, r)) => {
            content.push_str(unsafe { checker.get_unchecked(..l) });
            content.push_str(&task.const_name);
            content.push_str(unsafe { checker.get_unchecked(r..) });
        }
        None => content.push_str(&checker),
    }

    fs::write(format!("{sroot}/main.lean"), content)?;

    // deposit_one(&task.uid, &task.module_name, &[], &task.hash, &sroot)?;

    Ok(())
}

async fn deposit(task @ Task { sid, .. }: Task) -> Result<(), BoxedStdError> {
    {
        let mut conn = get_connection().await?;
        Submission::report_status(sid, SubmissionStatus::Depositing, &mut conn).await?;
    }

    tokio::task::spawn_blocking(|| deposit_inner(task)).await??;

    {
        let mut conn = get_connection().await?;
        Submission::report_status(sid, SubmissionStatus::Deposited, &mut conn).await.map_err(Into::into)
    }
}

pub async fn main() -> io::Result<!> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    TX.get_or_init(|| tx);

    while let Some(task @ Task { sid, .. }) = rx.recv().await {
        if let Err(e) = deposit(task).await {
            tracing::warn!("error deposit submission #{sid}: {e}");
            if let Ok(mut conn) = get_connection().await {
                let _ = Submission::report_status(sid, SubmissionStatus::JudgementFailed, &mut conn).await;
            }
        }
    }

    Err(io::const_error!(io::ErrorKind::BrokenPipe, "Channel was closed unexpectedly"))
}
