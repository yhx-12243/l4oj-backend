use std::time::SystemTime;

use compact_str::CompactString;
use serde::Serialize;
use tokio_postgres::Client;

use crate::libs::{db::DBResult, validate::check_uid};

#[derive(Serialize)]
pub struct User {
    #[serde(rename = "id")]
    pub uid: CompactString,
    pub username: CompactString,
    pub email: CompactString,
    #[serde(rename = "registrationTime")]
    pub register_time: SystemTime,
}

impl User {
    pub async fn by_uid(uid: &str, db: &mut Client) -> DBResult<Option<Self>> {
        pub const SQL: &str = "select uid, username, email, register_time from lean4oj.users where uid = $1 and username != ''";
        println!("check by uid {uid:?}");

        if !check_uid(uid) {
            return Ok(None);
        }

        let stmt = db.prepare_static(SQL.into()).await?;
        let Some(row) = db.query_opt(&stmt, &[&uid]).await? else {
            return Ok(None);
        };

        let uid = row.try_get::<_, &str>(0)?.into();
        let username = row.try_get::<_, &str>(1)?.into();
        let email = row.try_get::<_, &str>(2)?.into();
        let register_time = row.try_get(3)?;

        Ok(Some(Self { uid, username, email, register_time }))
    }
}
