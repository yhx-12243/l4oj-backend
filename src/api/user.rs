use axum::{Json, Router, routing::post};
use bytes::Bytes;
use compact_str::CompactString;
use http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::{libs::{db::get_connection, request::JsonReqult, response::JkmxJsonResponse, serde::{UnitMap, WithJson}}, models::user::User};

#[derive(Deserialize)]
struct GetUserListRequest {
    #[serde(rename = "skipCount")]
    skip: i64,
    #[serde(rename = "takeCount")]
    take: i64,
}

async fn get_user_list(req: JsonReqult<GetUserListRequest>) -> JkmxJsonResponse {
    let Json(GetUserListRequest { skip, take }) = req?;

    let mut conn = get_connection().await?;
    let users = User::list(skip, take, &mut conn).await?;
    let count = User::count(&mut conn).await?;

    let res = format!(r#"{{"count":{count},"userMetas":{}}}"#, WithJson(users));
    JkmxJsonResponse::Response(StatusCode::OK, res.into())
}

#[derive(Deserialize)]
struct GetUserDetailRequest {
    uid: CompactString,
}

#[derive(Serialize)]
struct GetUserDetailResponse {
    meta: User,
    information: UnitMap,
    submissionCountPerDay: [!; 0],
}

async fn get_user_detail(req: JsonReqult<GetUserDetailRequest>) -> JkmxJsonResponse {
    let Json(GetUserDetailRequest { uid }) = req?;

    let mut conn = get_connection().await?;
    let Some(user) = User::by_uid(&uid, &mut conn).await? else {
        return JkmxJsonResponse::Response(StatusCode::OK, const { Bytes::from_static(br#"{"error":"NO_SUCH_USER"}"#) });
    };

    let res = GetUserDetailResponse {
        meta: user,
        information: UnitMap {},
        submissionCountPerDay: [],
    };

    JkmxJsonResponse::Response(StatusCode::OK, serde_json::to_vec(&res)?.into())
}

pub fn router() -> Router {
    Router::new()
        .route("/getUserList", post(get_user_list))
        .route("/getUserDetail", post(get_user_detail))
}
