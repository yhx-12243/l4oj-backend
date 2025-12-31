#![allow(clippy::declare_interior_mutable_const)]

use core::time::Duration;

use http::header::{HeaderName, HeaderValue};
use bytes::Bytes;

pub const BYTES_NULL: Bytes = Bytes::from_static(b"null");
pub const FORWARDED_HOST: HeaderName = HeaderName::from_static("d579fff85ca2d5edeaa17f53ab008dc84eb292a5");
pub const REMOTE_ADDR: HeaderName = HeaderName::from_static("009f34034b761c32384fde345378c488efc18c59");
pub const X_ACCEL_REDIRECT: HeaderName = HeaderName::from_static("x-accel-redirect");
pub const DUMMY_HOST: HeaderValue = HeaderValue::from_static("kitsune.kiriha");
pub const APPLICATION_JAVASCRIPT_UTF_8: HeaderValue = HeaderValue::from_static("application/javascript; charset=utf-8");
pub const APPLICATION_JSON_UTF_8: HeaderValue = HeaderValue::from_static("application/json; charset=utf-8");
pub const APPLICATION_CBOR: HeaderValue = HeaderValue::from_static("application/cbor");

pub mod db {
    pub const HOST: &str = option_env!("DB_HOST").unwrap_or("/var/run/postgresql");
    pub const USER: &str = option_env!("DB_USER").unwrap_or("postgres");
    pub const DBNAME: &str = option_env!("DB_NAME").unwrap_or("postgres");
    pub const PASSWORD: Option<&str> = option_env!("DB_PASSWORD");
    pub const CONNECTION_TIMEOUT: core::time::Duration = core::time::Duration::from_secs(5);
}

pub const GLOBAL_INTERVAL: Duration = Duration::from_secs(
    #[cfg(debug_assertions)]
    60,
    #[cfg(not(debug_assertions))]
    600,
);

pub const SESSION_EXPIRE: Duration = Duration::from_hours(1);
