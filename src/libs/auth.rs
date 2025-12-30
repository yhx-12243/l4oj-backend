use core::{convert::Infallible, future::{Ready, ready}};
use std::time::SystemTime;

use axum::extract::FromRequestParts;
use base64::{Engine, prelude::BASE64_STANDARD};
use compact_str::CompactString;
use http::{header::AUTHORIZATION, request::Parts};

#[repr(transparent)]
pub struct Uid(pub Option<CompactString>);

impl<S> FromRequestParts<S> for Uid {
	type Rejection = Infallible;

	fn from_request_parts(parts: &mut Parts, _state: &S) -> Ready<Result<Self, Self::Rejection>> {
		ready(Ok(Uid(decode(parts))))
	}
}

fn decode(parts: &mut Parts) -> Option<CompactString> {
	let now = *parts.extensions.get::<SystemTime>()?;
	let header = parts.headers.get(AUTHORIZATION)?.as_bytes();
	let base64 = header.strip_prefix(b"Bearer ")?;
	let v = BASE64_STANDARD.decode(base64).ok()?;





	None
}
