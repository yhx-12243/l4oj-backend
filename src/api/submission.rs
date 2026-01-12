use std::time::SystemTime;

use axum::{Extension, Json, Router, routing::post};
use bytes::Bytes;
use compact_str::CompactString;
use http::{StatusCode, response::Parts};
use openssl::sha::Sha256;
use serde::Deserialize;

use crate::{
    bad, exs,
    libs::{
        auth::Session_, constants::BYTES_NULL, db::{DBError, get_connection}, olean, privilege,
        request::JsonReqult, response::JkmxJsonResponse, serde::WithJson, validate::is_lean_id,
    },
    models::{problem::Problem, submission::Submission}, service::submission_deposit,
};

mod private {
    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database submission error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
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

    let olean_path = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(&user.uid, &module_name);

    let Ok(olean) = tokio::fs::read(&*olean_path).await else { return EMPTY };
    let Some(meta) = olean::parse_meta(&olean) else { return EMPTY };
    let Some(consts) = olean::parse_consts(meta) else { return EMPTY };
    let Some(dependencies) = olean::parse_imports(meta) else { return EMPTY };

    let res = format!(r#"{{"consts":{},"dependencies":{}}}"#, WithJson(&*consts), WithJson(&*dependencies));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Inner1 {
    module_name: CompactString,
    const_name: CompactString,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubmitRequest {
    problem_id: i32,
    content: Inner1,
}

#[derive(Deserialize)]
pub struct Jb {
    checker: String,
}

async fn submit(
    Extension(now): Extension<SystemTime>,
    Session_(session): Session_,
    req: JsonReqult<SubmitRequest>,
) -> JkmxJsonResponse {
    const SQL_SEL_PRIV: &str = "select * from lean4oj.problems where pid = $1 and submittable";
    const SQL_SEL: &str = "select * from lean4oj.problems where pid = $1 and (owner = $2 or is_public) and submittable";
    const SQL_ADD_SUB: &str = "update lean4oj.problems set sub = sub + 1 where pid = $1";

    let Json(SubmitRequest { problem_id, content: Inner1 { module_name, const_name } }) = req?;

    if !module_name.split('.').all(is_lean_id) || !const_name.split('.').all(is_lean_id) { bad!(BYTES_NULL); }

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let problem: Problem = if privilege::check(&user.uid, "Lean4OJ.ManageProblem", &mut conn).await? {
        let stmt = conn.prepare(SQL_SEL_PRIV).await?;
        conn.query_one(&stmt, &[&problem_id]).await
    } else {
        let stmt = conn.prepare(SQL_SEL).await?;
        conn.query_one(&stmt, &[&problem_id, &&*user.uid]).await
    }?.try_into()?;

    let olean_path = olean::𝑔𝑒𝑡_𝑜𝑙𝑒𝑎𝑛_𝑝𝑎𝑡ℎ(&user.uid, &module_name);

    let olean = tokio::fs::read(&*olean_path).await?;
    let Some(meta) = olean::parse_meta(&olean) else { bad!(BYTES_NULL) };
    let Some(consts) = olean::parse_consts(meta) else { bad!(BYTES_NULL) };
    let Some(imports) = olean::parse_imports(meta) else { bad!(BYTES_NULL) };
    if !consts.contains(&const_name) { bad!(BYTES_NULL); }

    let mut sha256 = Sha256::new();
    sha256.update(&olean);
    let answer_hash = sha256.finish();

    let sid = Submission::create(problem_id, &user.uid, now,
        &module_name, &const_name, meta.version,
        olean.len() as u64, answer_hash,
        &mut conn,
    ).await?;

    let stmt = conn.prepare(SQL_ADD_SUB).await?;
    let n = conn.execute(&stmt, &[&problem_id]).await?;
    if n != 1 { return private::err(); }

    let task = submission_deposit::Task {
        sid,
        uid: user.uid,
        module_name,
        const_name,
        imports,
        hash: answer_hash,
        checker: serde_json::from_slice(&problem.jb).map(|Jb { checker }| checker),
    };
    submission_deposit::transmit(task)?;

    let res = format!(r#"{{"submissionId":{}}}"#, sid);
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

async fn query_submission() -> JkmxJsonResponse {
    let mut res = format!(r#"{{"submissions":[],"hasSmallerId":false,"hasLargerId":false"#);
    res.push('}');
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

pub fn router(_header: &'static Parts) -> Router {
    Router::new()
        .route("/getOleanMeta", post(get_olean_meta))
        .route("/submit", post(submit))
        .route("/querySubmission", post(query_submission))
}
