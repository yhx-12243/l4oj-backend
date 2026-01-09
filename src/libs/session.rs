use core::future::{Ready, ready};
use std::{sync::LazyLock, time::SystemTime};

use dashmap::DashMap;
use futures_util::FutureExt;
use hashbrown::DefaultHashBuilder;
use tower_sessions_core::{
    ExpiredDeletion, Expiry, Session, SessionStore,
    session::{Id, Record},
};

use super::constants::{GLOBAL_INTERVAL, SESSION_EXPIRE};

pub type SResult<T> = Result<T, tower_sessions_core::session::Error>;
pub type SSResult<T> = tower_sessions_core::session_store::Result<T>;

#[derive(Debug)]
pub struct GlobalStore;

static MAP: LazyLock<DashMap<Id, Record, DefaultHashBuilder>> = LazyLock::new(|| DashMap::with_hasher(DefaultHashBuilder::default()));

impl SessionStore for GlobalStore {
    fn create(&self, record: &mut Record) -> Ready<SSResult<()>> {
        while MAP.contains_key(&record.id) {
            record.id = Id::default();
        }
        MAP.insert(record.id, record.clone());
        ready(Ok(()))
    }

    fn save(&self, record: &Record) -> Ready<SSResult<()>> {
        MAP.insert(record.id, record.clone());
        ready(Ok(()))
    }

    fn load(&self, session_id: &Id) -> Ready<SSResult<Option<Record>>> {
        let s = MAP.get(session_id);
        let t = s
            .filter(|r| r.expiry_date >= SystemTime::now())
            .map(|r| r.clone());
        ready(Ok(t))
    }

    fn delete(&self, session_id: &Id) -> Ready<SSResult<()>> {
        MAP.remove(session_id);
        ready(Ok(()))
    }
}

impl ExpiredDeletion for GlobalStore {
    fn delete_expired(&self) -> Ready<SSResult<()>> {
        tracing::debug!(target: "expired-session-cleaner", "start clean");

        let cc1 = MAP.len();
        let now = SystemTime::now();
        for shard in MAP.shards() {
            if let Some(mut shard) = shard.try_write() {
                shard.retain(|v| v.1.expiry_date >= now);
            }
        }
        let cc2 = MAP.len();
        tracing::debug!(target: "expired-session-cleaner", "cleaned \x1b[32m{cc1}\x1b[0m -> \x1b[32m{cc2}\x1b[0m");

        ready(Ok(()))
    }
}

pub fn init() {
    tokio::spawn(GlobalStore.continuously_delete_expired(GLOBAL_INTERVAL).map(Result::unwrap));
}

pub async fn create(uid: String) -> SResult<Session<GlobalStore>> {
    let session = Session::new(
        None,
        GlobalStore,
        Some(Expiry::OnInactivity(SESSION_EXPIRE)),
    );
    session.insert_value("uid", serde_json::Value::String(uid)).await?;
    session.save().await?;
    Ok(session)
}

pub async fn load(id: Id) -> SResult<Session<GlobalStore>> {
    let session = Session::new(
        Some(id),
        GlobalStore,
        Some(Expiry::OnInactivity(SESSION_EXPIRE)),
    );
    session.load().await?;
    session.save().await?;
    Ok(session)
}

#[macro_export]
#[allow(unused_variables)]
macro_rules! exs {
    ($user:ident, $sess:expr, $db:expr) => {
        let Some($user) = $crate::models::user::User::from_maybe_session($sess, $db).await? else {
            return $crate::libs::response::JkmxJsonResponse::Response(
                http::StatusCode::UNAUTHORIZED,
                $crate::libs::constants::BYTES_NULL,
            );
        };
    }
}
