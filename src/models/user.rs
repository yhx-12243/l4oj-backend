use core::future::ready;
use std::time::SystemTime;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use serde::Serialize;
use tokio_postgres::{Client, Row};

use crate::libs::{
    db::{DBError, DBResult},
    serde::JsTime,
    validate::check_uid,
};

#[derive(Serialize)]
pub struct User {
    #[serde(rename = "id")]
    pub uid: CompactString,
    pub username: CompactString,
    pub email: CompactString,
    #[serde(rename = "registrationTime", serialize_with = "JsTime")]
    pub register_time: SystemTime,
    #[serde(rename = "acceptedProblemCount")]
    pub ac: u32,
}

impl TryFrom<Row> for User {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let uid = row.try_get::<_, &str>(0)?.into();
        let username = row.try_get::<_, &str>(1)?.into();
        let email = row.try_get::<_, &str>(2)?.into();
        let register_time = row.try_get(3)?;
        let ac = row.try_get::<_, i32>(4)?.cast_unsigned();
        Ok(Self { uid, username, email, register_time, ac })
    }
}

impl User {
    pub async fn by_uid(uid: &str, db: &mut Client) -> DBResult<Option<Self>> {
        pub const SQL: &str = "select uid, username, email, register_time, ac from lean4oj.users where uid = $1 and username != ''";

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

    pub async fn list(skip: i64, take: i64, db: &mut Client) -> DBResult<Vec<Self>> {
        pub const SQL: &str = "select uid, username, email, register_time, ac from lean4oj.users where username != '' order by ac desc, uid asc offset $1 limit $2";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, &[&skip, &take]).await?;
        stream.and_then(|row| ready(User::try_from(row))).try_collect().await
    }

    pub async fn count(db: &mut Client) -> DBResult<i64> {
        pub const SQL: &str = "select count(*) from lean4oj.users where username != ''";

        let stmt = db.prepare_static(SQL.into()).await?;
        let row = db.query_one(&stmt, &[]).await?;
        let count = row.try_get::<_, i64>(0)?;
        Ok(count)
    }
}
