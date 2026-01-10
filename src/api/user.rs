use axum::{
    Json, Router,
    extract::Query,
    routing::{get, post, post_service},
};
use bytes::Bytes;
use compact_str::CompactString;
use http::{StatusCode, response::Parts};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use smallvec::SmallVec;
use tokio_postgres::{Client, types::Json as QJson};

use crate::{
    bad, exs,
    libs::{
        auth::Session_,
        constants::{BYTES_EMPTY, BYTES_NULL, PASSWORD_LENGTH},
        db::{DBError, DBResult, JsonChecked, get_connection},
        lquery, privilege,
        request::{JsonReqult, RawPayload, Repult},
        response::JkmxJsonResponse,
        serde::WithJson,
        validate::{check_email, check_username},
    },
    models::user::{User, UserA, UserInformation},
};

const NO_SUCH_USER: JkmxJsonResponse = JkmxJsonResponse::Response(
    StatusCode::OK,
    Bytes::from_static(br#"{"error":"NO_SUCH_USER"}"#),
);

mod private {
    use futures_util::FutureExt;

    #[inline]
    pub(super) fn λ(src: &str, shortcut: bool, conn: &mut super::Client) -> impl Future<Output = super::DBResult<bool>> {
        if shortcut {
            core::future::ready(Ok(true)).left_future()
        } else {
            super::privilege::check(src, "Lean4OJ.ManageUser", conn).right_future()
        }
    }

    #[inline]
    pub(super) fn γ(src: &str, dest: &str, conn: &mut super::Client) -> impl Future<Output = super::DBResult<bool>> {
        λ(src, *src == *dest, conn)
    }

    pub(super) fn err() -> super::JkmxJsonResponse {
        let err = super::DBError::new(tokio_postgres::error::Kind::RowCount, Some("database update error".into()));
        return super::JkmxJsonResponse::Error(super::StatusCode::INTERNAL_SERVER_ERROR, err.into());
    }
}

#[derive(Deserialize)]
struct SearchUserRequest {
    query: CompactString,
}

async fn search_user(req: Repult<Query<SearchUserRequest>>) -> JkmxJsonResponse {
    let Query(SearchUserRequest { query }) = req?;

    let Some((dot, query)) = lquery::normalize(&query) else { bad!(BYTES_NULL) };

    let mut conn = get_connection().await?;
    let users = User::search(dot, &query, &mut conn).await?;

    let res = format!(r#"{{"userMetas":{}}}"#, WithJson(users));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetUserMetaRequest {
    uid: CompactString,
    get_privileges: Option<bool>,
}

#[derive(Serialize)]
struct GetUserMetaResponse {
    meta: UserA,
    privileges: privilege::Privileges,
}

async fn get_user_meta(req: JsonReqult<GetUserMetaRequest>) -> JkmxJsonResponse {
    let Json(GetUserMetaRequest { uid, get_privileges }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };

    let res = if get_privileges == Some(true) {
        let privileges = privilege::all(&user.uid, &mut conn).await?;
        GetUserMetaResponse {
            meta: UserA { user, is_admin: privilege::is_admin(&privileges) },
            privileges,
        }
    } else {
        GetUserMetaResponse {
            meta: UserA { user, is_admin: false },
            privileges: SmallVec::new(),
        }
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserProfileRequest {
    user_id: CompactString,
    username: CompactString,
    email: CompactString,
    avatar_info: CompactString,
    nickname: CompactString,
    bio: CompactString,
    information: UserInformation,
}

async fn update_user_profile(
    Session_(session): Session_,
    req: JsonReqult<UpdateUserProfileRequest>,
) -> JkmxJsonResponse {
    const SQL_UPDATE_USER: &str = "update lean4oj.users set username = $1, email = $2, avatar_info = $3, nickname = $4, bio = $5 where uid = $6";
    const SQL_UPDATE_INFORMATION: &str = "update lean4oj.user_information set organization = $1, location = $2, url = $3, telegram = $4, qq = $5, github = $6 where uid = $7";

    let Json(UpdateUserProfileRequest { user_id, username, email, avatar_info, nickname, bio, information }) = req?;

    if !check_username(&username) { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    exs!(s_user, &session, &mut conn);
    let Some(t_user) = User::by_uid(&user_id, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt_update_user = conn.prepare_static(SQL_UPDATE_USER.into()).await?;
    let stmt_update_information = conn.prepare_static(SQL_UPDATE_INFORMATION.into()).await?;
    let txn = conn.transaction().await?;
    let n = txn.execute(&stmt_update_user, &[&&*username, &&*email, &&*avatar_info, &&*nickname, &&*bio, &&*t_user.uid]).await?;
    if n != 1 { return private::err() }
    let n = txn.execute(&stmt_update_information, &[&&*information.organization, &&*information.location, &&*information.url, &&*information.telegram, &&*information.qq, &&*information.github, &&*t_user.uid]).await?;
    if n != 1 { return private::err() }
    txn.commit().await?;

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetUserListRequest {
    skip_count: u64,
    take_count: u64,
}

async fn get_user_list(req: JsonReqult<GetUserListRequest>) -> JkmxJsonResponse {
    let Json(GetUserListRequest { skip_count, take_count }) = req?;

    let skip = skip_count.min(i64::MAX.cast_unsigned()).cast_signed();
    let take = take_count.min(100).cast_signed();

    let mut conn = get_connection().await?;
    let users = User::list(skip, take, &mut conn).await?;
    let count = User::count(&mut conn).await?;

    let res = format!(r#"{{"userMetas":{},"count":{count}}}"#, WithJson(users));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
struct GetSingleUserRequest {
    uid: CompactString,
}

#[derive(Serialize)]
struct GetUserDetailResponse {
    meta: User,
    information: UserInformation,
    submissionCountPerDay: [!; 0],
    rank: u64,
    hasPrivilege: bool,
}

async fn get_user_detail(req: JsonReqult<GetSingleUserRequest>) -> JkmxJsonResponse {
    const SQL_RANK: &str = "select count(*) from lean4oj.users where ac > $1";

    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    let stmt = conn.prepare_static(SQL_RANK.into()).await?;
    let row = conn.query_one(&stmt, &[&user.ac.cast_signed()]).await?;
    let information = UserInformation::of(&uid, &mut conn).await?;

    let res = GetUserDetailResponse {
        meta: user,
        information,
        submissionCountPerDay: [],
        rank: row.try_get::<_, i64>(0)?.cast_unsigned() + 1,
        hasPrivilege: true,
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GetUserProfileResponse {
    meta: User,
    information: UserInformation,
    public_email: bool,
    avatar_info: CompactString,
}

async fn get_user_profile(req: JsonReqult<GetSingleUserRequest>) -> JkmxJsonResponse {
    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    let information = UserInformation::of(&uid, &mut conn).await?;

    let avatar_info = user.avatar_info.clone();
    let res = GetUserProfileResponse {
        meta: user,
        information,
        public_email: true,
        avatar_info,
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

async fn get_user_preference(
    Session_(session): Session_,
    req: JsonReqult<GetSingleUserRequest>,
) -> JkmxJsonResponse {
    const SQL_GET_PREF: &str = "select preference from lean4oj.user_preference where uid = $1";

    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, &session, &mut conn);
    let Some(t_user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL_GET_PREF.into()).await?;
    let row = conn.query_one(&stmt, &[&&*t_user.uid]).await?;
    let pref = row.try_get::<_, JsonChecked>(0)?;

    let res = format!(r#"{{"meta":{},"preference":{}}}"#, WithJson(t_user), unsafe { core::str::from_utf8_unchecked(pref.0) });
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserPreferenceRequest {
    user_id: CompactString,
    preference: serde_json::Map<String, Value>,
}

async fn update_user_preference(
    Session_(session): Session_,
    req: JsonReqult<UpdateUserPreferenceRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.user_preference set preference = $1 where uid = $2";

    let Json(UpdateUserPreferenceRequest { user_id, preference }) = req?;

    let mut conn = get_connection().await?;
    exs!(s_user, &session, &mut conn);
    let Some(t_user) = User::by_uid(&user_id, &mut conn).await? else { return NO_SUCH_USER };
    if !private::γ(&s_user.uid, &t_user.uid, &mut conn).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&QJson(preference), &&*t_user.uid]).await?;
    if n != 1 { return private::err() }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

async fn get_user_security_settings(req: JsonReqult<GetSingleUserRequest>) -> JkmxJsonResponse {
    let Json(GetSingleUserRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else { return NO_SUCH_USER };

    let res = format!(r#"{{"meta":{}}}"#, WithJson(user));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

const fn query_audit_logs(header: &'static Parts) -> RawPayload {
    RawPayload { header, body: br#"{"count":0,"results":[]}"# }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdatePasswordRequest {
    user_id: CompactString,
    old_password: Option<CompactString>,
    password: CompactString,
}

async fn update_password(
    Session_(session): Session_,
    req: JsonReqult<UpdatePasswordRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.users set password = $1 where uid = $2";

    let Json(UpdatePasswordRequest { user_id, old_password, password }) = req?;

    if password.len() != PASSWORD_LENGTH || !password.is_ascii() { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    exs!(s_user, &session, &mut conn);
    let Some(t_user) = User::by_uid(&user_id, &mut conn).await? else { return NO_SUCH_USER };
    if !private::λ(
        &s_user.uid,
        *s_user.uid == *t_user.uid && old_password.is_some_and(|p| *p.as_bytes() == t_user.password),
        &mut conn,
    ).await? {
        return JkmxJsonResponse::Response(StatusCode::FORBIDDEN, BYTES_NULL);
    }

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&&*password, &&*t_user.uid]).await?;
    if n != 1 { return private::err() }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

#[derive(Deserialize)]
struct UpdateEmailRequest {
    email: CompactString,
}

async fn update_email(
    Session_(session): Session_,
    req: JsonReqult<UpdateEmailRequest>,
) -> JkmxJsonResponse {
    const SQL: &str = "update lean4oj.users set email = $1 where uid = $2";

    let Json(UpdateEmailRequest { email }) = req?;

    if check_email(&email).is_none() { bad!(BYTES_NULL) }

    let mut conn = get_connection().await?;
    exs!(user, &session, &mut conn);

    let stmt = conn.prepare_static(SQL.into()).await?;
    let n = conn.execute(&stmt, &[&&*email, &&*user.uid]).await?;
    if n != 1 { return private::err() }

    JkmxJsonResponse::Response(StatusCode::OK, BYTES_EMPTY)
}

pub fn router(header: &'static Parts) -> Router {
    Router::new()
        .route("/searchUser", get(search_user))
        .route("/getUserMeta", post(get_user_meta))
        .route("/updateUserProfile", post(update_user_profile))
        .route("/getUserList", post(get_user_list))
        .route("/getUserDetail", post(get_user_detail))
        .route("/getUserProfile", post(get_user_profile))
        .route("/getUserPreference", post(get_user_preference))
        .route("/updateUserPreference", post(update_user_preference))
        .route("/getUserSecuritySettings", post(get_user_security_settings))
        .route("/queryAuditLogs", post_service(query_audit_logs(header)))
        .route("/updateUserPassword", post(update_password))
        .route("/updateUserSelfEmail", post(update_email))
}
