use core::{
    convert::Infallible,
    fmt,
    future::{Ready, ready},
    pin::Pin,
    sync::atomic::{AtomicU64, Ordering},
    task::{Context, Poll, ready},
};
use std::time::SystemTime;

use axum::{
    Json, Router,
    body::Body,
    extract::{FromRequest, FromRequestParts},
    response::Response,
    routing::future::RouteFuture,
};
use bytes::Bytes;
use http::{
    Request,
    header::{REFERER, USER_AGENT},
    response::Parts,
};
use hyper::{
    body::{Body as _, Incoming},
    service::Service,
};

use super::constants::REMOTE_ADDR;

pub type Repult<T> = Result<T, <T as FromRequestParts<()>>::Rejection>;
pub type Reqult<T> = Result<T, <T as FromRequest<()>>::Rejection>;
pub type JsonReqult<T> = Reqult<Json<T>>;

#[repr(transparent)]
pub struct RouterService(pub Router<()>);

impl Service<Request<Incoming>> for RouterService {
    type Response = Response;
    type Error = !;
    type Future = ResponseFuture;

    fn call(&self, mut req: Request<Incoming>) -> Self::Future {
        static NONCE: AtomicU64 = AtomicU64::new(0);
        let t = SystemTime::now();
        let nonce = NONCE.fetch_add(1, Ordering::Relaxed);
        tracing::info!(
            "\x1b[35m<Request \x1b[1;34m{nonce}\x1b[35m>\x1b[0m {:?} \"{} {} {:?}\" {:?} {:?}",
            req.headers().get(REMOTE_ADDR),
            req.method(), req.uri(), req.version(),
            req.headers().get(REFERER), req.headers().get(USER_AGENT),
        );
        req.extensions_mut().insert(t);
        ResponseFuture(self.0.call_with_state(req.map(Body::new), ()), nonce, t)
    }
}

pub struct ResponseFuture(RouteFuture<Infallible>, u64, SystemTime);

impl Future for ResponseFuture {
    type Output = Result<Response, !>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Ok(res) = ready!(unsafe { self.as_mut().map_unchecked_mut(|x| &mut x.0) }.poll(cx));

        tracing::info!(
            "\x1b[35m<Response \x1b[1;34m{}\x1b[35m>\x1b[0m {:?} {} {:?}",
            self.1, res.status(), SizeHint(res.body().size_hint()), unsafe { self.2.elapsed().unwrap_unchecked() },
        );

        Poll::Ready(Ok(res))
    }
}

#[repr(transparent)]
pub struct SizeHint(hyper::body::SizeHint);

impl fmt::Display for SizeHint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.0.lower(), self.0.upper()) {
            (0, None) => f.write_str("/"),
            (x, None) => {
                f.write_str("≥")?;
                x.fmt(f)
            }
            (l, Some(r)) if l == r => l.fmt(f),
            (l, Some(r)) => {
                f.write_str("[")?;
                l.fmt(f)?;
                f.write_str(", ")?;
                r.fmt(f)?;
                f.write_str("]")
            }
        }
    }
}

#[derive(Clone, Copy)]
pub struct RawPayload {
    pub header: &'static Parts,
    pub body: &'static [u8],
}

impl tower_service::Service<Request<Body>> for RawPayload {
    type Response = Response;
    type Future = Ready<Result<Response, Infallible>>;
    type Error = Infallible;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _req: Request<Body>) -> Self::Future {
        ready(Ok(Response::from_parts(
            self.header.clone(),
            Bytes::from_static(self.body).into(),
        )))
    }
}
