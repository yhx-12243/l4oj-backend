use core::future::ready;
use std::time::SystemTime;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use serde::{Serialize, Serializer, ser::SerializeMap};
use tokio_postgres::{Client, Row};

use crate::{
    libs::{
        db::{DBError, DBResult},
        util::get_millis,
    },
    models::{discussion::DiscussionReactionAOE, user::UserAOE},
};

#[derive(Debug)]
pub struct Reply {
    pub id: u32,
    pub content: CompactString,
    pub publish: SystemTime,
    pub edit: SystemTime,
    pub did: u32,
    pub publisher: CompactString,
}

impl Serialize for Reply {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("content", &self.content)?;
        map.serialize_entry("publishTime", &get_millis(self.publish))?;
        if self.publish != self.edit {
            map.serialize_entry("editTime", &get_millis(self.edit))?;
        }
        map.serialize_entry("isPublic", &true)?;
        map.serialize_entry("discussionId", &self.did)?;
        map.serialize_entry("publisherId", &self.publisher)?;
        map.end()
    }
}

impl TryFrom<Row> for Reply {
    type Error = DBError;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let id = row.try_get::<_, i32>("id")?.cast_unsigned();
        let content = row.try_get::<_, &str>("content")?.into();
        let publish = row.try_get("publish")?;
        let edit = row.try_get("edit")?;
        let did = row.try_get::<_, i32>("did")?.cast_unsigned();
        let publisher = row.try_get::<_, &str>("publisher")?.into();
        Ok(Self { id, content, publish, edit, did, publisher })
    }
}

impl Reply {
    pub async fn stat_head_tail(did: u32, head: u64, tail: u64, db: &mut Client) -> DBResult<Vec<Self>> {
        const SQL: &str = "(select id, content, publish, edit, did, publisher from lean4oj.discussion_replies where did = $1 order by id limit $2::integer) union (select * from lean4oj.discussion_replies where did = $1 order by id desc limit $3::integer) order by id";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, [did.cast_signed(), head as i32, tail as i32]).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }

    pub async fn stat_interval(did: u32, before: u32, after: u32, count: u64, db: &mut Client) -> DBResult<Vec<Self>> {
        const SQL: &str = "select id, content, publish, edit, did, publisher from lean4oj.discussion_replies where did = $1 and id > $2 and id < $3 order by id limit $4::integer";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, [did.cast_signed(), after.cast_signed(), before.cast_signed(), count as i32]).await?;
        stream.and_then(|row| ready(Self::try_from(row))).try_collect().await
    }
}

#[derive(Serialize)]
pub struct ReplyAOE<'a> {
    #[serde(flatten)]
    pub reply: &'a Reply,
    pub publisher: Option<&'a UserAOE>,
    pub reactions: &'a DiscussionReactionAOE,
    pub permissions: [&'static str; 3],
}

pub const PERMISSION_DEFAULT: [&str; 3] = ["Modify", "ManagePublicness", "Delete"];
