use axum::{Router, routing::get};
use http::{StatusCode, response::Parts};
use serde::{Serialize, Serializer, ser::SerializeSeq};
use serde_json::ser::Serializer as JSerializer;

use crate::{
    libs::{db::get_connection, response::JkmxJsonResponse},
    models::group::AUV,
};

#[derive(Serialize)]
struct IdAndName<'a> {
    id: &'a str,
    name: &'a str,
}

async fn list_judge_clients() -> JkmxJsonResponse {
    let mut conn = get_connection().await?;
    let l = AUV::list("Lean4OJ.Judger", &mut conn).await?;

    let mut buf = r#"{"judgeClients":"#.to_owned();
    let mut ser = JSerializer::new(unsafe { buf.as_mut_vec() });
    let mut seq = ser.serialize_seq(Some(l.len()))?;
    for AUV { user_meta, .. } in &l {
        seq.serialize_element(&IdAndName { id: &user_meta.uid, name: &user_meta.username })?;
    }
    seq.end()?;
    buf.push_str(r#","hasManagePermission":true}"#);

    JkmxJsonResponse::Response(StatusCode::OK, buf.into())
}

pub fn router(_header: &'static Parts) -> Router {
    Router::new().route("/listJudgeClients", get(list_judge_clients))
}
