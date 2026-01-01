use axum::{
    body::Body,
    extract::RawPathParams,
    response::{IntoResponse, Response},
};
use http::{HeaderMap, HeaderValue, StatusCode, header::AUTHORIZATION};

use crate::libs::{auth::Session_, constants::X_ACCEL_REDIRECT, fs};

pub async fn static_with_permission(
    header: HeaderMap,
    params: RawPathParams,
    Session_(session): Session_,
) -> Response {
    let Some((deref!("path"), path)) = params.into_iter().next() else {
        return (StatusCode::INTERNAL_SERVER_ERROR, "Invalid route (proxy).").into_response();
    };
    if fs::is_read_forbidden(&path, session).await {
        return (
            if header.contains_key(AUTHORIZATION) {
                (StatusCode::FORBIDDEN, "You don't have permission to view this olean file.")
            } else {
                (StatusCode::UNAUTHORIZED, "You should provide a Bearer Authorization Header.")
            }
        ).into_response()
    }
    let mut res = Response::new(Body::empty());
    res.headers_mut().insert(X_ACCEL_REDIRECT, const { HeaderValue::from_static("@lean") });
    res
}
