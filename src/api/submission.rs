use axum::{Json, Router, routing::post};
use bytes::Bytes;
use compact_str::CompactString;
use http::{StatusCode, response::Parts};
use serde::Deserialize;

use crate::{
    bad, exs,
    libs::{
        auth::Session_, constants::BYTES_NULL, db::get_connection, olean, request::JsonReqult,
        response::JkmxJsonResponse, serde::WithJson, validate::is_lean_id,
    },
};

mod private {
    const ROOT: &str = &env!("OLEAN_ROOT")[..env!("OLEAN_ROOT").len() - 3];

    pub(super) fn 𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(uid: &str, name: &str) -> String {
        let mut s = String::with_capacity(env!("OLEAN_ROOT").len() + uid.len() + name.len() + 4);
        s.push_str(ROOT);
        s.push_str(uid);
        for part in name.split('.') {
            s.push('/');
            s.push_str(part);
        }
        s.push_str(".olean");
        s
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetOleanMetaRequest {
    module_name: CompactString,
}

async fn get_olean_meta(
    Session_(session): Session_,
    req: JsonReqult<GetOleanMetaRequest>,
) -> JkmxJsonResponse {
    const EMPTY: JkmxJsonResponse = JkmxJsonResponse::Response(StatusCode::OK, Bytes::from_static(br#"{"consts":[],"dependencies":[]}"#));

    let Json(GetOleanMetaRequest { module_name }) = req?;

    if !module_name.split('.').all(is_lean_id) { bad!(BYTES_NULL); }

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let olean_path = private::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(&user.uid, &module_name);

    let Ok(olean) = tokio::fs::read(&*olean_path).await else { return EMPTY };
    let Some(meta) = olean::parse_meta(&olean) else { return EMPTY };
    let Some(consts) = olean::parse_consts(meta) else { return EMPTY };
    let Some(dependencies) = olean::parse_imports(meta) else { return EMPTY };

    let res = format!(r#"{{"consts":{},"dependencies":{}}}"#, WithJson(&*consts), WithJson(&*dependencies));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetDependenciesRequest {
    module_name: CompactString,
    const_name: CompactString,
}

pub fn router(_header: &'static Parts) -> Router {
    Router::new()
        .route("/getOleanMeta", post(get_olean_meta))
        // .route("/submit", post(submit))
}
