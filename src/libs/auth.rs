use core::{convert::Infallible, mem, ptr};
use std::{fs, sync::OnceLock};

use axum::extract::FromRequestParts;
use base64::{Engine, prelude::BASE64_STANDARD};
use futures_util::{FutureExt, future::Map};
use http::{header::AUTHORIZATION, request::Parts};
use openssl::{bn::BigNum, ec::EcKey, ecdsa::EcdsaSig, pkey::Private};
use tower_sessions_core::{Session, session::Id};

use super::session::{self, GlobalStore};

#[repr(transparent)]
pub struct Session_(pub Option<Session<GlobalStore>>);

impl<S> FromRequestParts<S> for Session_ {
    type Rejection = Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        _state: &S,
    ) -> Map<
        impl Future<Output = Option<Session<GlobalStore>>>,
        fn(Option<Session<GlobalStore>>) -> Result<Self, Infallible>,
    > {
        decode(parts).map(|s| Ok(Self(s)))
    }
}

async fn decode(parts: &Parts) -> Option<Session<GlobalStore>> {
    let header = parts.headers.get(AUTHORIZATION)?.as_bytes();
    let base64 = header.strip_prefix(b"Bearer ")?;
    let encoded = Encoded::try_from(base64).ok()?;
    if !encoded.verify() { return None }
    session::load(encoded.id).await.ok()
}

#[repr(C)]
pub struct Encoded {
    pub id: Id,
    r: [u8; 48],
    s: [u8; 48],
}

impl Encoded {
    pub fn verify(&self) -> bool {
        let Ok(r) = BigNum::from_slice(&self.r) else { return false };
        let Ok(s) = BigNum::from_slice(&self.s) else { return false };
        let Ok(sign) = EcdsaSig::from_private_components(r, s) else { return false };
        matches!(
            sign.verify(
                &self.id.0.to_be_bytes(),
                #[cfg(feature = "build-std")]
                unsafe { ECKEY.get_unchecked() },
                #[cfg(not(feature = "build-std"))]
                unsafe { ECKEY.get().unwrap_unchecked() },
            ),
            Ok(true),
        )
    }
}

impl TryFrom<Id> for Encoded {
    type Error = openssl::error::ErrorStack;

    fn try_from(id: Id) -> Result<Self, Self::Error> {
        let sign = EcdsaSig::sign(
            &id.0.to_be_bytes(),
            #[cfg(feature = "build-std")]
            unsafe { ECKEY.get_unchecked() },
            #[cfg(not(feature = "build-std"))]
            unsafe { ECKEY.get().unwrap_unchecked() },
        )?;
        let raw_sign: *const openssl_sys::ECDSA_SIG = unsafe { mem::transmute_copy(&sign) };
        let mut r0 = ptr::null();
        let mut s0 = ptr::null();
        let mut r = [0u8; 48];
        let mut s = [0u8; 48];
        unsafe {
            openssl_sys::ECDSA_SIG_get0(raw_sign, &raw mut r0, &raw mut s0);
            openssl_sys::BN_bn2binpad(r0, r.as_mut_ptr(), 48);
            openssl_sys::BN_bn2binpad(s0, s.as_mut_ptr(), 48);
        }
        Ok(Self { id, r, s })
    }
}

impl TryFrom<&[u8]> for Encoded {
    type Error = ();

    fn try_from(src: &[u8]) -> Result<Self, Self::Error> {
        const N: usize = mem::size_of::<Encoded>();
        let mut buf = [0u8; N];
        if BASE64_STANDARD.decode_slice(src, &mut buf) == Ok(N) {
            Ok(unsafe { mem::transmute::<[u8; N], Self>(buf) })
        } else {
            Err(())
        }
    }
}

static ECKEY: OnceLock<EcKey<Private>> = OnceLock::new();

pub fn init() {
    const PRIVATE_KEY_PATH: &str = "/usr/local/nginx/conf/private.key";

    let key_pem = fs::read(PRIVATE_KEY_PATH).unwrap();
    ECKEY.get_or_init(|| EcKey::private_key_from_pem(&key_pem).unwrap());
}
