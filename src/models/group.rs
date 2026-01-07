use core::future::ready;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use serde::{Serialize, ser::SerializeStruct};
use tokio_postgres::{Client, Row, types::ToSql};

use crate::{
    libs::db::{DBError, DBResult},
    models::user::User,
};

pub struct Group {
    pub gid: CompactString,
    pub member_count: u32,
}

impl Serialize for Group {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Group", 3)?;
        state.serialize_field("id", &*self.gid)?;
        state.serialize_field("name", &*self.gid)?;
        state.serialize_field("memberCount", &self.member_count)?;
        state.end()
    }
}

impl TryFrom<Row> for Group {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let gid = row.try_get::<_, &str>(0)?.into();
        let member_count = row.try_get::<_, i32>(1)?.cast_unsigned();
        Ok(Self { gid, member_count })
    }
}

impl Group {
    pub async fn list(db: &mut Client) -> DBResult<Vec<Self>> {
        const SQL: &str = "select gid, member_count from lean4oj.groups order by gid";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, core::iter::empty::<&dyn ToSql>()).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }
}

pub struct GroupA {
    pub group: Group,
    pub is_admin: bool,
}

impl Serialize for GroupA {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.group.serialize(serializer)
    }
}

impl TryFrom<Row> for GroupA {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let gid = row.try_get::<_, &str>(0)?.into();
        let member_count = row.try_get::<_, i32>(1)?.cast_unsigned();
        let is_admin = row.try_get(2)?;
        Ok(Self { group: Group { gid, member_count }, is_admin })
    }
}

impl GroupA {
    pub async fn list(uid: &str, db: &mut Client) -> DBResult<Vec<Self>> {
        const SQL: &str = "select groups.gid, member_count, coalesce(is_admin, false) from lean4oj.groups left join lean4oj.user_groups on groups.gid = user_groups.gid and uid = $1 order by groups.gid";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, [uid]).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }

    pub async fn count(uid: &str, db: &mut Client) -> DBResult<u64> {
        const SQL: &str = "select count(*) from lean4oj.user_groups where uid = $1";

        let stmt = db.prepare_static(SQL.into()).await?;
        let row = db.query_one(&stmt, &[&uid]).await?;
        Ok(row.try_get::<_, i64>(0)?.cast_unsigned())
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AUV {
    pub user_meta: User,
    pub is_group_admin: bool,
}

impl TryFrom<Row> for AUV {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let user_meta = row.clone().try_into()?;
        let is_group_admin = row.try_get(9)?;
        Ok(Self { user_meta, is_group_admin })
    }
}

impl AUV {
    pub async fn list(gid: &str, db: &mut Client) -> DBResult<Vec<Self>> {
        const SQL: &str = "select users.uid, password, username, email, register_time, ac, nickname, bio, avatar_info, is_admin from lean4oj.users inner join lean4oj.user_groups on users.uid = user_groups.uid and gid = $1 order by users.uid";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, [gid]).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }
}
