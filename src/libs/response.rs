use core::{convert::Infallible, ops::FromResidual};

use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use http::{StatusCode, header};

use super::{
    constants::APPLICATION_JSON_UTF_8,
    db::{BB8Error, DBError},
    error::{BoxedStdError, serialize_err},
};

pub enum JkmxJsonResponse {
    Response(StatusCode, Bytes),
    Error(StatusCode, BoxedStdError),
}

impl IntoResponse for JkmxJsonResponse {
    fn into_response(self) -> Response {
        let (status_code, body) = match self {
            Self::Response(c, b) => (c, b),
            Self::Error(c, e) => (c, serialize_err(&*e).into()),
        };
        let mut res = Response::new(body.into());
        *res.status_mut() = status_code;
        res.headers_mut().insert(header::CONTENT_TYPE, APPLICATION_JSON_UTF_8);
        res
    }
}

impl FromResidual<Self> for JkmxJsonResponse {
    #[inline(always)]
    fn from_residual(residual: Self) -> Self {
        residual
    }
}

macro_rules! impl_lsz {
    ($ty:ty) => {
        impl FromResidual<Result<Infallible, $ty>> for JkmxJsonResponse {
            fn from_residual(Err(err): Result<Infallible, $ty>) -> Self {
                Self::Error(StatusCode::BAD_REQUEST, err.into())
            }
        }
    };
    ($ty:ty, $st:expr) => {
        impl FromResidual<Result<Infallible, $ty>> for JkmxJsonResponse {
            fn from_residual(Err(err): Result<Infallible, $ty>) -> Self {
                Self::Error($st, err.into())
            }
        }
    };
}

impl_lsz!(BoxedStdError);
impl_lsz!(serde::de::value::Error);
impl_lsz!(serde_json::Error);
impl_lsz!(core::str::Utf8Error);
impl_lsz!(std::io::Error, StatusCode::INTERNAL_SERVER_ERROR);

impl_lsz!(core::fmt::Error);
impl_lsz!(DBError, StatusCode::INTERNAL_SERVER_ERROR);
impl_lsz!(BB8Error, StatusCode::INTERNAL_SERVER_ERROR);
impl_lsz!(http::Error);
impl_lsz!(hyper::Error);
impl_lsz!(tokio::task::JoinError, StatusCode::INTERNAL_SERVER_ERROR);

macro_rules! impl_zsxn {
    ($ty:ty) => {
        impl FromResidual<Result<Infallible, $ty>> for JkmxJsonResponse {
            fn from_residual(Err(err): Result<Infallible, $ty>) -> Self {
                Self::Error(err.status(), err.into())
            }
        }
    };
}

impl_zsxn!(axum::extract::rejection::JsonRejection);
impl_zsxn!(axum::extract::rejection::PathRejection);
impl_zsxn!(axum::extract::rejection::QueryRejection);

#[macro_export]
macro_rules! bad {
    ($expr:expr) => {
        return $crate::libs::response::JkmxJsonResponse::Response(
            http::StatusCode::BAD_REQUEST,
            $expr.into(),
        )
    };
}
