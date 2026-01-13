use bytes::Buf;
use http::{Request, header};
use http_body_util::BodyExt;
use hyper::client::conn::{self, http1::SendRequest};

#[path = "../libs/judger/task.rs"]
mod __task;
pub use __task::Task;

use crate::constants::{APPLICATION_JSON_UTF_8, DUMMY_HOST, PASSWORD, USERNAME};

pub async fn get(sender: &mut SendRequest<String>) -> hyper::Result<serde_json::Result<Task>> {
    let req = Request::post("/api/submission/judger__get__task")
        .header(header::HOST, DUMMY_HOST)
        .header(header::CONTENT_TYPE, APPLICATION_JSON_UTF_8)
        .body(format!(r#"{{"uid":"{USERNAME}","password":"{PASSWORD}"}}"#))
        .unwrap();

    let res = sender
        .try_send_request(req)
        .await
        .map_err(conn::TrySendError::into_error)?;

    let body = res.into_body().collect().await?;
    let reader = body.aggregate().reader();
    Ok(serde_json::from_reader(reader))
}
