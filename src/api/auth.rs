use core::{mem, panic, slice};
use std::time::SystemTime;

use axum::{
    Extension, Json, Router,
    body::Body,
    extract::Query,
    response::{IntoResponse, Response},
    routing::{get, post, post_service},
};
use base64::{display::Base64Display, prelude::BASE64_STANDARD};
use bytes::Bytes;
use compact_str::CompactString;
use http::{StatusCode, Uri, header, response::Parts};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use tower_sessions_core::session::Id;

use crate::{
    bad,
    libs::{
        auth::{Encoded, Session_, availability},
        constants::{
            APPLICATION_JAVASCRIPT_UTF_8, APPLICATION_JSON_UTF_8, BYTES_NULL, PASSWORD_LENGTH,
        },
        db::{DBError, JsonChecked, get_connection},
        preference::server::PreferenceConfig,
        privilege,
        request::{JsonReqult, RawPayload, Repult},
        response::JkmxJsonResponse,
        session,
        validate::{check_email, check_uid, check_username},
    },
    models::{
        group::GroupA,
        user::{User, UserA},
    },
};

mod private {
    use serde_json::{Serializer as JSerializer, ser::CompactFormatter};
    use std::io::Write;

    pub(super) trait Δ: serde::Serializer {
        fn δ(_: &*const [u8], _: Self) -> Result<Self::Ok, Self::Error>;
    }

    impl<S: serde::Serializer> Δ for S {
        default fn δ(_: &*const [u8], _: Self) -> Result<Self::Ok, Self::Error> {
            // Won't be instantiated.
            unimplemented!("Not implemented intentionally.");
        }
    }

    impl Δ for &mut JSerializer<&mut Vec<u8>, CompactFormatter> {
        fn δ(data: &*const [u8], serializer: Self) -> Result<Self::Ok, Self::Error> {
            serializer.as_inner().0.write_all(unsafe { &**data }).map_err(serde_json::Error::io)
        }
    }

    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database insertion error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }
}

#[derive(Deserialize)]
struct SessionInfoRequest {
    jsonp: Option<CompactString>,
    token: Option<String>,
}

#[derive(Serialize)]
struct ServerVersion {
    hash: &'static str,
    date: u64,
}

impl const Default for ServerVersion {
    fn default() -> Self {
        Self {
            hash: env!("SERVER_VERSION_HASH"),
            date: const {
                if let Ok(date) = u64::from_str_radix(env!("SERVER_VERSION_DATE"), 10) {
                    date * 1000
                } else {
                    panic!("Invalid SERVER_VERSION_DATE");
                }
            },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionInfoResponse {
    server_preference: PreferenceConfig,
    server_version: ServerVersion,
    user_meta: Option<UserA>,
    joined_groups_count: Option<u64>,
    user_privileges: privilege::Privileges,
    #[serde(serialize_with = "private::Δ::δ")]
    user_preference: *const [u8],
}

unsafe impl Send for SessionInfoResponse {}

async fn get_session_info(req: Repult<Query<SessionInfoRequest>>) -> Response {
    const JSONP_HEAD: &str = "(globalThis.getSessionInfoCallback??(e=>globalThis.sessionInfo=e))(";
    const JSONP_TRAIL: &str = ");";
    const SQL_GET_PREF: &str = "select preference from lean4oj.user_preference where uid = $1";
    const EMPTY: &[u8] = b"{}";

    fn not_falsy(inner: CompactString) -> bool {
        !["false", "f", "no", "n", "off", "0"]
            .into_iter()
            .any(|s| inner.eq_ignore_ascii_case(s))
    }

    let Query(SessionInfoRequest { jsonp, token }) = match req {
        Ok(s) => s,
        Err(err) => return err.into_response(),
    };
    let jsonp = jsonp.is_some_and(not_falsy);

    let mut res = SessionInfoResponse {
        server_version: const { ServerVersion::default() },
        server_preference: const { PreferenceConfig::default() },
        user_meta: None,
        joined_groups_count: None,
        user_privileges: SmallVec::new(),
        user_preference: core::ptr::from_ref(EMPTY),
    };

    if let Some(token) = token
    && let Ok(encoded) = Encoded::try_from(token.as_bytes())
    && encoded.verify()
    && let Ok(session) = session::load(encoded.id).await
    && let Ok(mut conn) = get_connection().await
    && let Ok(Some(user)) = User::from_session(&session, &mut conn).await {
        res.joined_groups_count = GroupA::count(&user.uid, &mut conn).await.ok();
        res.user_privileges = privilege::all(&user.uid, &mut conn).await.unwrap_or_default();
        if let Ok(stmt) = conn.prepare_static(SQL_GET_PREF.into()).await
        && let Ok(row) = conn.query_one(&stmt, &[&&*user.uid]).await
        && let Ok(pref) = row.try_get::<_, JsonChecked>(0) {
            res.user_preference = pref.0;
        }
        res.user_meta = Some(UserA { user, is_admin: privilege::is_admin(&res.user_privileges) });
    }

    let mut body = if jsonp { JSONP_HEAD.to_owned() } else { String::new() };
    let _ = serde_json::to_writer(unsafe { body.as_mut_vec() }, &res);
    if jsonp { body.push_str(JSONP_TRAIL); }

    let mut res = Response::new(Body::from(body));
    res.headers_mut().insert(
        header::CONTENT_TYPE,
        if jsonp { APPLICATION_JAVASCRIPT_UTF_8 } else { APPLICATION_JSON_UTF_8 },
    );
    res
}

#[derive(Deserialize)]
struct LoginRequest {
    identifier: Option<CompactString>,
    email: Option<CompactString>,
    password: String,
}

async fn login(req: JsonReqult<LoginRequest>) -> JkmxJsonResponse {
    const SQL_ID: &str = "select uid, username from lean4oj.users where uid = $1 and username != '' and password = $2";
    const SQL_EMAIL: &str = "select uid, username from lean4oj.users where username != '' and email = $1 and password = $2";

    let Json(LoginRequest { identifier, email, password }) = req?;
    if identifier.is_none() && email.is_none() { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    let row = if let Some(id) = identifier {
        let stmt = conn.prepare_static(SQL_ID.into()).await?;
        conn.query_one(&stmt, &[&&*id, &password]).await
    } else {
        let email = unsafe { email.unwrap_unchecked() };
        let stmt = conn.prepare_static(SQL_EMAIL.into()).await?;
        conn.query_one(&stmt, &[&&*email, &password]).await
    };
    let row = match row {
        Ok(r) => r,
        Err(e) => return JkmxJsonResponse::Error(StatusCode::BAD_REQUEST, e.into()),
    };

    let uid = row.try_get(0)?;
    let username = row.try_get::<_, &str>(1)?;
    let session = session::create(uid).await?;
    let encoded = Encoded::try_from(session.id().unwrap_or(Id(0)))?;
    let bytes: &[u8] = unsafe { slice::from_raw_parts((&raw const encoded).cast(), mem::size_of::<Encoded>()) };
    let res = format!(r#"{{"token":"{}","username":"{username}"}}"#, Base64Display::new(bytes, &BASE64_STANDARD));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

async fn logout(Session_(session): Session_) -> JkmxJsonResponse {
    if let Some(session) = session {
        session.delete().await?;
    }
    JkmxJsonResponse::Response(StatusCode::OK, BYTES_NULL)
}

async fn check_availability(req: Uri) -> JkmxJsonResponse {
    let Some(query) = req.query() else { return JkmxJsonResponse::Response(StatusCode::OK, BYTES_NULL) };

    let res = match form_urlencoded::parse(query.as_bytes()).next() {
        Some((deref!("username"), _)) => const { Bytes::from_static(br#"{"usernameAvailable":true}"#) },
        Some((deref!("identifier"), id)) => {
            let mut conn = get_connection().await?;
            let a = availability::identifier(&id, &mut conn).await?;
            format!(r#"{{"identifierAvailable":{a}}}"#).into()
        }
        Some((deref!("email"), email)) => {
            let mut conn = get_connection().await?;
            let a = availability::email(&email, &mut conn).await?;
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
    const SQL_USERS: &str = "insert into lean4oj.users (uid, username, email, password, register_time, avatar_info) values ($1, $2, $3::text, $4, $5, 'gravatar:' || $3::text)";
    const SQL_USER_INFORMATION: &str = "insert into lean4oj.user_information (uid) values ($1)";
    const SQL_USER_PREFERENCE: &str = "insert into lean4oj.user_preference (uid) values ($1)";

    let Json(RegisterRequest {
        username,
        identifier,
        email,
        password,
    }) = req?;

    if !check_username(&username) || !check_uid(&identifier) || check_email(&email).is_none() || password.len() != PASSWORD_LENGTH || !password.is_ascii() {
        bad!(BYTES_NULL)
    }

    let mut conn = get_connection().await?;
    let stmt_users = conn.prepare_static(SQL_USERS.into()).await?;
    let stmt_user_information = conn.prepare_static(SQL_USER_INFORMATION.into()).await?;
    let stmt_user_preference = conn.prepare_static(SQL_USER_PREFERENCE.into()).await?;
    let txn = conn.transaction().await?;
    let n = txn.execute(&stmt_users, &[&&*identifier, &&*username, &&*email, &&*password, &now]).await?;
    if n != 1 { return private::err() }
    let n = txn.execute(&stmt_user_information, &[&&*identifier]).await?;
    if n != 1 { return private::err() }
    let n = txn.execute(&stmt_user_preference, &[&&*identifier]).await?;
    if n != 1 { return private::err() }
    txn.commit().await?;

    let session = session::create(identifier.into_string()).await?;
    let encoded = Encoded::try_from(session.id().unwrap_or(Id(0)))?;
    let bytes: &[u8] = unsafe { slice::from_raw_parts((&raw const encoded).cast(), mem::size_of::<Encoded>()) };
    let res = format!(r#"{{"token":"{}"}}"#, Base64Display::new(bytes, &BASE64_STANDARD));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

const fn list_user_sessions(header: &'static Parts) -> RawPayload {
    RawPayload { header, body: br#"{"sessions":[]}"# }
}

pub fn router(header: &'static Parts) -> Router {
    Router::new()
        .route("/getSessionInfo", get(get_session_info))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/checkAvailability", get(check_availability))
        .route("/register", post(register))
        .route("/listUserSessions", post_service(list_user_sessions(header)))
}
