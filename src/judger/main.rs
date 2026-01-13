use core::slice;
use std::{borrow::Cow, io, process::Stdio};

use http::{Request, header};
use hyper::{
    client::conn::{self, http1::SendRequest},
    rt::{Read, Write},
};
use serde::Serialize;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

use crate::{
    constants::{APPLICATION_JSON_UTF_8, DUMMY_HOST, PASSWORD, USERNAME},
    task,
};

#[path = "../models/submission/message.rs"]
mod message;
#[path = "../models/submission/status.rs"]
mod status;

#[derive(Serialize)]
struct Report<'a> {
    uid: &'a str,
    password: &'a str,
    sid: u32,
    status: status::Status,
    message: message::Action,
    answer: Option<&'a str>,
}

pub async fn report(
    sid: u32,
    status: status::Status,
    message: message::Action,
    answer: Option<&str>,
    sender: &mut SendRequest<String>,
) -> io::Result<()> {
    tracing::debug!("[submission #{sid}] status = {status:?}, message = {message:?}, answer = {answer:?}");

    let s = Report {
        uid: USERNAME,
        password: PASSWORD,
        sid,
        status,
        message,
        answer,
    };
    let req = Request::post("/api/submission/judger__report__status")
        .header(header::HOST, DUMMY_HOST)
        .header(header::CONTENT_TYPE, APPLICATION_JSON_UTF_8)
        .body(serde_json::to_string(&s)?)
        .unwrap();

    match sender.try_send_request(req).await {
        Ok(_) => Ok(()),
        Err(e) => Err(io::Error::other(e.into_error())),
    }
}

pub async fn read_string<R>(reader: &mut R) -> io::Result<String>
where
    R: AsyncRead + Unpin,
{
    let len = reader.read_u32_le().await?;
    let mut buf = String::with_capacity(len as usize);
    reader.read_exact(unsafe { slice::from_raw_parts_mut(buf.as_mut_ptr(), len as usize) }).await?;
    unsafe { buf.as_mut_vec().set_len(len as usize); }
    Ok(buf)
}

pub async fn main_loop<S>(sock: S) -> io::Result<!>
where
    S: Read + Write + Send + Unpin + 'static,
{
    let (mut sender, conn) = conn::http1::handshake::<_, String>(sock)
        .await
        .map_err(io::Error::other)?;
    let _conn_backend = tokio::spawn(conn.with_upgrades());

    let l4judger = format!("{}/l4judger", env!("OLEAN_ROOT"));

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;

        let task = match task::get(&mut sender).await {
            Ok(Ok(t)) => t,
            Ok(Err(e)) => { tracing::warn!("Failed to deserialize task: {e}"); continue; }
            Err(e) => { tracing::warn!("Failed to get task: {e}"); continue; }
        };

        tracing::debug!("Received task: {task:?}");

        let bytes = task.sid.to_le_bytes();
        let lean_path = format!(
            "{0}/leanprover--lean4---v{2}/lib/lean:{1}/std/{2}:{1}/lean/Lean4OJ:{1}/submissions/{6:02x}/{5:02x}/{4:02x}/{3:02x}",
            env!("LEAN4_TOOLCHAIN_DIR"),
            env!("OLEAN_ROOT"),
            task.version,
            bytes[0], bytes[1], bytes[2], bytes[3],
        );
        let arg = format!(
            "{}/submissions/{:02x}/{:02x}/{:02x}/{:02x}/main.lean",
            env!("OLEAN_ROOT"), bytes[3], bytes[2], bytes[1], bytes[0],
        );

        let mut cmd = Command::new(&*l4judger);
        cmd.env("LEAN_PATH", lean_path);
        cmd.arg(arg);
        cmd.args(task.axioms);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
        #[cfg(target_os = "linux")]
        cmd.uid(0xdeadbeef);
        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to spawn l4judger: {e}");
                let _ = report(task.sid, status::Status::JudgementFailed, message::Action::Replace(Cow::Owned(e.to_string())), None, &mut sender).await;
                continue;
            }
        };
        let mut stdout = child.stdout.take().unwrap();

        // main loop
        while let Ok(status_raw) = stdout.read_u8().await
           && let Ok(status) = status::Status::try_from(status_raw)
           && let Ok(message_raw) = stdout.read_u8().await {
            let message = match message_raw {
                0 => message::Action::NoAction,
                1 => {
                    let Ok(s) = read_string(&mut stdout).await else { break };
                    message::Action::Replace(Cow::Owned(s))
                }
                2 => {
                    let Ok(s) = read_string(&mut stdout).await else { break };
                    message::Action::Append(Cow::Owned(s))
                }
                _ => break,
            };
            let Ok(has_answer) = stdout.read_u8().await else { break };
            let answer = match has_answer {
                0 => None,
                1 => {
                    let Ok(s) = read_string(&mut stdout).await else { break };
                    Some(s)
                }
                _ => break,
            };
            let _ = report(task.sid, status, message, answer.as_deref(), &mut sender).await;
        }

        let err = match child.wait().await {
            Ok(status) => match status.exit_ok() {
                Ok(()) => None,
                Err(e) => Some(e.to_string())
            },
            Err(e) => Some(e.to_string()),
        };

        if let Some(err) = err {
            tracing::warn!("l4judger process failed: {err}");
            let _ = report(task.sid, status::Status::JudgementFailed, message::Action::Replace(Cow::Owned(err)), None, &mut sender).await;
        }
    }
}
