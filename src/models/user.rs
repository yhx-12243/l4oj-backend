use core::future::ready;
use std::time::SystemTime;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use serde::Serialize;
use serde_json::Value;
use tokio_postgres::{Client, Row};
use tower_sessions_core::Session;

use crate::libs::{
    constants::PASSWORD_LENGTH,
    db::{DBError, DBResult},
    serde::JsTime,
    session::GlobalStore,
    validate::check_uid,
};

mod information;
pub use information::Information as UserInformation;

#[derive(Serialize)]
pub struct User {
    #[serde(rename = "id")]
    pub uid: CompactString,
    #[serde(skip)]
    pub password: [u8; PASSWORD_LENGTH],
    pub username: CompactString,
    pub email: CompactString,
    #[serde(rename = "registrationTime", serialize_with = "JsTime")]
    pub register_time: SystemTime,
    #[serde(rename = "acceptedProblemCount")]
    pub ac: u32,
    pub nickname: CompactString,
    pub bio: CompactString,
    #[serde(rename = "avatar")]
    pub avatar_info: CompactString,
}

impl TryFrom<Row> for User {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let uid = row.try_get::<_, &str>("uid")?.into();
        let password = row.try_get::<_, &str>("password")?.as_bytes();
        let password = password.try_into().map_err(|e|
            DBError::new(tokio_postgres::error::Kind::FromSql(1), Some(Box::new(e)))
        )?;
        let username = row.try_get::<_, &str>("username")?.into();
        let email = row.try_get::<_, &str>("email")?.into();
        let register_time = row.try_get("register_time")?;
        let ac = row.try_get::<_, i32>("ac")?.cast_unsigned();
        let nickname = row.try_get::<_, &str>("nickname")?.into();
        let bio = row.try_get::<_, &str>("bio")?.into();
        let avatar_info = row.try_get::<_, &str>("avatar_info")?.into();
        Ok(Self { uid, password, username, email, register_time, ac, nickname, bio, avatar_info })
    }
}

impl User {
    pub async fn by_uid(uid: &str, db: &mut Client) -> DBResult<Option<Self>> {
        pub const SQL: &str = "select uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.users where uid = $1 and username != ''";

        if !check_uid(uid) {
            return Ok(None);
        }

        let stmt = db.prepare_static(SQL.into()).await?;
        let result = match db.query_opt(&stmt, &[&uid]).await? {
            Some(row) => Some(row.try_into()?),
            None => None,
        };
        Ok(result)
    }

    pub async fn from_session(session: &Session<GlobalStore>, db: &mut Client) -> DBResult<Option<Self>> {
        let Ok(Some(Value::String(uid))) = session.get_value("uid").await else { return Ok(None) };
        Self::by_uid(&uid, db).await
    }

    #[allow(clippy::ref_option)]
    pub async fn from_maybe_session(session: &Option<Session<GlobalStore>>, db: &mut Client) -> DBResult<Option<Self>> {
        if let Some(session) = session
        && let Ok(Some(Value::String(uid))) = session.get_value("uid").await {
            Self::by_uid(&uid, db).await
        } else {
            Ok(None)
        }
    }

    pub async fn list(skip: i64, take: i64, db: &mut Client) -> DBResult<Vec<Self>> {
        pub const SQL: &str = "select uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.users where username != '' order by ac desc, uid offset $1 limit $2";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, [skip, take]).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }

    pub async fn search(uid: bool, query: &str, db: &mut Client) -> DBResult<Vec<Self>> {
        pub const SQL_UID: &str = "select uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.users where username != '' and uid like $1 order by ac desc, uid limit 10";
        pub const SQL_USERNAME: &str = "select uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.users where username != '' and username ilike $1 order by ac desc, uid limit 10";

        let stmt = db.prepare_static(if uid { SQL_UID } else { SQL_USERNAME }.into()).await?;
        let stream = db.query_raw(&stmt, [query]).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }

    pub async fn count(db: &mut Client) -> DBResult<u64> {
        pub const SQL: &str = "select count(*) from lean4oj.users where username != ''";

        let stmt = db.prepare_static(SQL.into()).await?;
        let row = db.query_one(&stmt, &[]).await?;
        row.try_get::<_, i64>(0).map(i64::cast_unsigned)
    }
}

#[derive(Serialize)]
pub struct UserA {
    #[serde(flatten)]
    pub user: User,
    #[serde(rename = "isAdmin")]
    pub is_admin: bool,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAOE {
    #[serde(flatten)]
    pub user: User,
    pub is_admin: bool,
    pub is_problem_admin: bool,
    pub is_contest_admin: bool,
    pub is_discussion_admin: bool,
}
