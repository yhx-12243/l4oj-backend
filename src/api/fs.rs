use axum::{
    body::Body,
    extract::RawPathParams,
    response::{IntoResponse, Response},
};
use bytes::Bytes;
use http::{HeaderValue, StatusCode};

use crate::{
    libs::{
        auth::Session_,
        constants::{X_ACCEL_KITSUNE, X_ACCEL_REDIRECT},
        db::{DBResult, get_connection},
        privilege,
    },
    models::user::User,
};

pub async fn submission(
    Session_(session): Session_,
    params: RawPathParams,
) -> Response {
    const PRIVIS: [&str; 3] = ["Lean4OJ.Admin", "Lean4OJ.Judger", "Lean4OJ.ManageProblem"];

    let Some((deref!("path"), path)) = params.into_iter().next() else { return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid route (proxy).").into_response() };

    let pos = path.find('/').unwrap_or(path.len());
    let id_str = unsafe { path.get_unchecked(..pos) };
    let Ok(sid) = id_str.parse::<u32>() else { return StatusCode::NOT_FOUND.into_response() };

    let Ok(mut conn) = get_connection().await else { return (StatusCode::INTERNAL_SERVER_ERROR, "Database not available.").into_response() };

    let e: DBResult<()> = try {
        const SQL_USER: &str = "select from lean4oj.submissions natural join lean4oj.problems where sid = $1 and (owner = $2 or is_public)";
        const SQL_GUEST: &str = "select from lean4oj.submissions natural join lean4oj.problems where sid = $1 and is_public";

        if let Some(user) = User::from_maybe_session(&session, &mut conn).await? {
            if !privilege::check_any(&user.uid, PRIVIS.into_iter(), &mut conn).await? {
                let stmt = conn.prepare_static(SQL_USER.into()).await?;
                if conn.query_opt(&stmt, &[&sid.cast_signed(), &&*user.uid]).await?.is_none() {
                    return StatusCode::NOT_FOUND.into_response();
                }
            }
        } else {
            let stmt = conn.prepare_static(SQL_GUEST.into()).await?;
            if conn.query_opt(&stmt, &[&sid.cast_signed()]).await?.is_none() {
                return StatusCode::NOT_FOUND.into_response();
            }
        }
    };
    if let Err(e) = e { return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(); }

    let bytes = sid.to_le_bytes();
    let suffix = unsafe { path.get_unchecked(pos..) };
    let redirect = format!(
        "/internal-bf9d9f9f9f1b0b0f/{:02x}/{:02x}/{:02x}/{:02x}{suffix}",
        bytes[3], bytes[2], bytes[1], bytes[0],
    );
    let kitsune = format!("/lean/submission/{sid}{suffix}");

    let mut res = Response::new(Body::empty());
    res.headers_mut().insert(
        X_ACCEL_REDIRECT,
        unsafe { HeaderValue::from_maybe_shared_unchecked(Bytes::from(redirect)) },
    );
    res.headers_mut().insert(
        X_ACCEL_KITSUNE,
        unsafe { HeaderValue::from_maybe_shared_unchecked(Bytes::from(kitsune)) },
    );
    res
}
