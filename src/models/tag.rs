use core::future::ready;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use hashbrown::HashMap;
use serde::Serialize;
use tokio_postgres::{
    Client, Row,
    types::{Json, ToSql},
};

use super::localedict::LocaleDict;
use crate::libs::db::{DBError, DBResult};

#[derive(Serialize)]
pub struct Tag {
    pub id: u32,
    pub color: CompactString,
    #[serde(rename = "localizedNames")]
    pub name: LocaleDict,
}

impl TryFrom<Row> for Tag {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let id = row.try_get::<_, i32>(0)?.cast_unsigned();
        let color = row.try_get::<_, &str>(1)?.into();
        let Json(name) = row.try_get(2)?;
        Ok(Self { id, color, name })
    }
}

impl Tag {
    pub async fn list(db: &mut Client) -> DBResult<Vec<Self>> {
        pub const SQL: &str = "select id, color, name from lean4oj.tags order by id";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, core::iter::empty::<&dyn ToSql>()).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }

    pub async fn create(color: &str, name: &LocaleDict, db: &mut Client) -> DBResult<u32> {
        pub const SQL: &str = "insert into lean4oj.tags (color, name) values ($1, $2) returning id";

        let stmt = db.prepare_static(SQL.into()).await?;
        let name: *const Json<HashMap<CompactString, CompactString>> = (&raw const name.0).cast();
        let row = db.query_one(&stmt, &[&color, unsafe { &*name }]).await?;
        row.try_get(0).map(i32::cast_unsigned)
    }
}
