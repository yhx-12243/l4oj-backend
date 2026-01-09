use core::{fmt, future::ready};
use std::time::SystemTime;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use serde::{Serialize, Serializer, ser::SerializeMap};
use smallvec::{SmallVec, smallvec};
use tokio_postgres::{Client, Row, types::ToSql};

use crate::{
    libs::{
        db::{DBResult, ToSqlIter},
        util::get_millis,
    },
    models::{localedict::LocaleDict, user::User},
};

mod query_replies_type;
mod reaction;
mod reply;
pub use query_replies_type::QueryRepliesType;
pub use reaction::{
    ReactionAOE as DiscussionReactionAOE, ReactionType as DiscussionReactionType,
    get_area_of_effect as reaction_aoe,
};
pub use reply::{
    PERMISSION_DEFAULT as REPLY_PERMISSION_DEFAULT, Reply as DiscussionReply,
    ReplyAOE as DiscussionReplyAOE,
};

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

#[inline]
fn ℊ(row: Row) -> DBResult<(Discussion, User)> {
    Ok((row.clone().try_into()?, row.try_into()?))
}

// example: {"zh_CN":"喵","en_US":"Meow","ja_JP":"にゃー"}
fn backdoor_inner(s: &mut CompactString, locale: Option<&str>) {
    if let Some(suffix) = s.strip_prefix(Discussion::MAGIC_PREFIX)
    && let Ok(dict) = serde_json::from_str::<LocaleDict>(suffix)
    && let Some(t) = dict.apply_owned(locale) {
        *s = t;
    }
}

impl Discussion {
    pub const MAGIC_PREFIX: &str = "\u{ea97}";

    pub async fn by_id_aoe(id: u32, db: &mut Client) -> DBResult<Option<(Self, User)>> {
        const SQL: &str = "select id, title, content, publish, edit, update, reply_count, publisher, uid, username, email, password, register_time, ac, nickname, bio, avatar_info from lean4oj.discussions inner join lean4oj.users on publisher = uid where id = $1";

        let stmt = db.prepare_static(SQL.into()).await?;
        let result = match db.query_opt(&stmt, &[&id.cast_signed()]).await? {
            Some(row) => Some(ℊ(row)?),
            None => None,
        };
        Ok(result)
    }

    pub async fn by_ids<I>(ids: I, db: &mut Client) -> DBResult<Vec<Self>>
    where
        I: ExactSizeIterator<Item = u32> + Clone + fmt::Debug + Sync,
    {
        const SQL: &str = "select ids.id, title, content, publish, edit, update, reply_count, publisher from unnest($1::integer[]) with ordinality as ids(id, o) inner join lean4oj.discussions on ids.id = discussions.id order by o";

        let stmt = db.prepare_static(SQL.into()).await?;
        let stream = db.query_raw(&stmt, [ToSqlIter(ids.map(u32::cast_signed))]).await?;
        stream.and_then(|row| ready(row.try_into())).try_collect().await
    }

    pub async fn create(title: &str, content: &str, time: SystemTime, publisher: &str, db: &mut Client) -> DBResult<u32> {
        const SQL: &str = "insert into lean4oj.discussions (title, content, publish, edit, update, publisher) values ($1, $2, $3, $3, $3, $4) returning id";

        let stmt = db.prepare_static(SQL.into()).await?;
        let row = db.query_one(&stmt, &[&title, &content, &time, &publisher]).await?;
        row.try_get(0).map(i32::cast_unsigned)
    }

    pub async fn search<'a, F>(skip: u64, take: u64, extend: F, db: &mut Client) -> DBResult<Vec<Self>>
    where
        F: FnOnce(String, SmallVec<[&'a (dyn ToSql + Sync); 8]>) -> (String, SmallVec<[&'a (dyn ToSql + Sync); 8]>),
    {
        let skip = skip.min(i64::MAX.cast_unsigned()).cast_signed();
        let take = take.min(100).cast_signed();
        let mut sql = "select id, title, content, publish, edit, update, reply_count, publisher from lean4oj.discussions".to_owned();
        let mut args: SmallVec<[&(dyn ToSql + Sync); 8]> = smallvec![
            unsafe { core::mem::transmute::<&i64, &'a i64>(&skip) } as _,
            unsafe { core::mem::transmute::<&i64, &'a i64>(&take) } as _,
        ];
        (sql, args) = extend(sql, args);
        sql.push_str(" order by update desc offset $1 limit $2");

        let stmt = db.prepare_static(sql.into()).await?;
        let stream = db.query_raw(&stmt, args).await?;
        stream.and_then(|row| ready(row.try_into())).try_collect().await
    }

    pub async fn search_aoe<'a, F>(skip: u64, take: u64, extend: F, db: &mut Client) -> DBResult<Vec<(Self, User)>>
    where
        F: FnOnce(String, SmallVec<[&'a (dyn ToSql + Sync); 8]>) -> (String, SmallVec<[&'a (dyn ToSql + Sync); 8]>),
    {
        let skip = skip.min(i64::MAX.cast_unsigned()).cast_signed();
        let take = take.min(100).cast_signed();
        let mut sql = "select id, title, content, publish, edit, update, reply_count, publisher, uid, username, email, password, register_time, ac, nickname, bio, avatar_info from lean4oj.discussions inner join lean4oj.users on publisher = uid".to_owned();
        let mut args: SmallVec<[&(dyn ToSql + Sync); 8]> = smallvec![
            unsafe { core::mem::transmute::<&i64, &'a i64>(&skip) } as _,
            unsafe { core::mem::transmute::<&i64, &'a i64>(&take) } as _,
        ];
        (sql, args) = extend(sql, args);
        sql.push_str(" order by update desc offset $1 limit $2");

        let stmt = db.prepare_static(sql.into()).await?;
        let stream = db.query_raw(&stmt, args).await?;
        stream.and_then(|row| ready(ℊ(row))).try_collect().await
    }

    pub async fn count<'a, F>(extend: F, db: &mut Client) -> DBResult<u64>
    where
        F: FnOnce(String, SmallVec<[&'a (dyn ToSql + Sync); 8]>) -> (String, SmallVec<[&'a (dyn ToSql + Sync); 8]>),
    {
        let mut sql = "select count(*) from lean4oj.discussions".to_owned();
        let mut args = SmallVec::new();
        (sql, args) = extend(sql, args);

        let stmt = db.prepare_static(sql.into()).await?;
        let row = db.query_one(&stmt, &args).await?;
        row.try_get::<_, i64>(0).map(i64::cast_unsigned)
    }

    pub fn backdoor(&mut self, locale: Option<&str>) {
        backdoor_inner(&mut self.title, locale);
        backdoor_inner(&mut self.content, locale);
    }
}
