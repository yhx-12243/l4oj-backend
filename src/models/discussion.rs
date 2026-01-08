use std::time::SystemTime;

use compact_str::CompactString;
use serde::{Serialize, Serializer, ser::SerializeMap};
use tokio_postgres::{Client, Row};

use crate::libs::{db::DBResult, util::get_millis};

mod query_replies_type;
mod reaction;
mod reply;
pub use query_replies_type::QueryRepliesType;
pub use reaction::{Reaction as DiscussionReaction, ReactionAOE as DiscussionReactionAOE};
pub use reply::{PERMISSION_DEFAULT, Reply as DiscussionReply, ReplyAOE as DiscussionReplyAOE};

pub struct Discussion {
    pub id: u32,
    pub title: CompactString,
    pub content: CompactString,
    pub publish: SystemTime,
    pub edit: SystemTime,
    pub update: SystemTime,
    pub reply_count: u32,
    pub publisher: CompactString,
}

impl Serialize for Discussion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id)?;
        map.serialize_entry("title", &self.title)?;
        map.serialize_entry("publishTime", &get_millis(self.publish))?;
        if self.publish != self.edit {
            map.serialize_entry("editTime", &get_millis(self.edit))?;
        }
        map.serialize_entry("sortTime", &get_millis(self.update))?;
        map.serialize_entry("replyCount", &self.reply_count)?;
        map.serialize_entry("isPublic", &true)?;
        map.serialize_entry("publisherId", &self.publisher)?;
        map.end()
    }
}

impl TryFrom<Row> for Discussion {
    type Error = tokio_postgres::Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let id = row.try_get::<_, i32>("id")?.cast_unsigned();
        let title = row.try_get::<_, &str>("title")?.into();
        let content = row.try_get::<_, &str>("content")?.into();
        let publish = row.try_get("publish")?;
        let edit = row.try_get("edit")?;
        let update = row.try_get("update")?;
        let reply_count = row.try_get::<_, i32>("reply_count")?.cast_unsigned();
        let publisher = row.try_get::<_, &str>("publisher")?.into();
        Ok(Self { id, title, content, publish, edit, update, reply_count, publisher })
    }
}

impl Discussion {
    pub async fn by_id(id: u32, db: &mut Client) -> DBResult<Self> {
        const SQL: &str = "select id, title, content, publish, edit, update, reply_count, publisher from lean4oj.discussions where id = $1";

        let stmt = db.prepare_static(SQL.into()).await?;
        db.query_one(&stmt, &[&id.cast_signed()]).await?.try_into()
    }

    pub async fn by_id_aoe(id: u32, db: &mut Client) -> DBResult<Option<(Self, crate::models::user::User)>> {
        const SQL: &str = "select id, title, content, publish, edit, update, reply_count, publisher, uid, username, email, password, register_time, ac, nickname, bio, avatar_info from lean4oj.discussions inner join lean4oj.users on publisher = uid where id = $1";

        let stmt = db.prepare_static(SQL.into()).await?;
        let result = match db.query_opt(&stmt, &[&id.cast_signed()]).await? {
            Some(row) => Some((row.clone().try_into()?, row.try_into()?)),
            None => None,
        };
        Ok(result)
    }

    pub async fn create(title: &str, content: &str, time: SystemTime, publisher: &str, db: &mut Client) -> DBResult<u32> {
        const SQL: &str = "insert into lean4oj.discussions (title, content, publish, edit, update, publisher) values ($1, $2, $3, $3, $3, $4) returning id";

        let stmt = db.prepare_static(SQL.into()).await?;
        let row = db.query_one(&stmt, &[&title, &content, &time, &publisher]).await?;
        row.try_get(0).map(i32::cast_unsigned)
    }
}
