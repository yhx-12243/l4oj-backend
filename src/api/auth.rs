use std::time::SystemTime;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::Query,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use compact_str::CompactString;
use http::{StatusCode, Uri, header};
use serde::{Deserialize, Serialize};

use crate::{
    bad,
    libs::{
        constants::{APPLICATION_JAVASCRIPT_UTF_8, APPLICATION_JSON_UTF_8, BYTES_NULL},
        db::{BB8Error, DBError, get_connection},
        preference::server::PreferenceConfig,
        request::{JsonReqult, Repult},
        response::JkmxJsonResponse,
        validate::{check_email, check_uid, check_username},
    },
};

#[derive(Deserialize)]
struct SessionInfoRequest {
    jsonp: Option<CompactString>,
    token: Option<String>,
}

#[derive(Serialize)]
struct ServerVersion {
    hash: &'static str,
    date: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfoResponse {
    server_version: ServerVersion,
    server_preference: PreferenceConfig,
    // userMeta: Option<_>,
    // joinedGroupsCount: Option<_>,
    // userPrivileges: Option<_>,
    // userPreference: Option<_>,
}

async fn get_session_info(req: Repult<Query<SessionInfoRequest>>) -> Response {
    const JSONP_HEAD: &str = "(globalThis.getSessionInfoCallback??(e=>globalThis.sessionInfo=e))(";
    const JSONP_TRAIL: &str = ");";
    const INFO: SessionInfoResponse = SessionInfoResponse {
        server_version: ServerVersion {
            hash: env!("SERVER_VERSION_HASH"),
            date: env!("SERVER_VERSION_DATE"),
        },
        server_preference: PreferenceConfig::default(),
    };

    fn not_falsy(inner: CompactString) -> bool {
        !["false", "f", "no", "n", "off", "0"]
            .into_iter()
            .any(|s| inner.eq_ignore_ascii_case(s))
    }

    let req = match req {
        Ok(Query(s)) => s,
        Err(err) => return err.into_response(),
    };
    let jsonp = req.jsonp.is_some_and(not_falsy);

    let mut body = if jsonp { JSONP_HEAD.to_owned() } else { String::new() };
    let _ = serde_json::to_writer(unsafe { body.as_mut_vec() }, &INFO);
    if jsonp { body.push_str(JSONP_TRAIL); }

    let mut res = Response::new(Body::from(body));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        if jsonp { APPLICATION_JAVASCRIPT_UTF_8 } else { APPLICATION_JSON_UTF_8 },
    );
    res
}

async fn check_identifier_availability(id: &str) -> Result<bool, BB8Error> {
    const SQL: &str = "select 1 from lean4oj.users where uid = $1";

    let mut conn = get_connection().await?;
    let stmt = conn.prepare_static(SQL.into()).await?;
    Ok(conn.query_opt(&stmt, &[&id]).await?.is_none())
}

async fn check_email_availability(email: &str) -> Result<bool, BB8Error> {
    const SQL: &str = "select 1 from lean4oj.users where email = $1";

    let mut conn = get_connection().await?;
    let stmt = conn.prepare_static(SQL.into()).await?;
    Ok(conn.query_opt(&stmt, &[&email]).await?.is_none())
}

async fn check_availability(req: Uri) -> JkmxJsonResponse {
    let Some(query) = req.query() else { return JkmxJsonResponse::Response(StatusCode::OK, BYTES_NULL) };

    let res = match form_urlencoded::parse(query.as_bytes()).next() {
        Some((deref!("username"), _)) => const { Bytes::from_static(br#"{"usernameAvailable":true}"#) },
        Some((deref!("identifier"), id)) => {
            let a = check_identifier_availability(&id).await?;
            format!(r#"{{"identifierAvailable":{a}}}"#).into()
        }
        Some((deref!("email"), email)) => {
            let a = check_email_availability(&email).await?;
            format!(r#"{{"emailAvailable":{a}}}"#).into()
        }
        _ => BYTES_NULL,
    };

    JkmxJsonResponse::Response(StatusCode::OK, res)
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: CompactString,
    identifier: CompactString,
    email: CompactString,
    password: String,
}

async fn register(
    Extension(now): Extension<SystemTime>,
    req: JsonReqult<RegisterRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "insert into lean4oj.users (uid, username, email, password, register_time) values ($1, $2, $3, $4, $5)";

    let Json(RegisterRequest {
        username,
        identifier,
        email,
        password,
    }) = req?;

    if !check_username(&username) || !check_uid(&identifier) || check_email(&email).is_none() {
        bad!(BYTES_NULL)
    }

    let mut conn = get_connection().await?;
    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(
        &stmt,
        &[&&*identifier, &&*username, &&*email, &&*password, &now],
    ).await?;
    if n != 1 {
        let err = DBError::new(tokio_postgres::error::Kind::RowCount, Some("database insertion error".into()));
        return JkmxJsonResponse::Error(StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_NULL)
}

pub fn router() -> Router {
    Router::new()
        .route("/getSessionInfo", get(get_session_info))
        .route("/checkAvailability", get(check_availability))
        .route("/register", post(register))
}
