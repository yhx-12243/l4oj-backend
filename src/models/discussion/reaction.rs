use core::{fmt, future::ready};

use compact_str::CompactString;
use futures_util::TryStreamExt;
use hashbrown::HashMap;
use serde::{Deserialize, Serialize};
use tokio_postgres::{Client, types::ToSql};

use crate::libs::db::{DBResult, ToSqlIterUnsafe};

#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ReactionType {
    Discussion,
    DiscussionReply,
}

#[derive(Default, Serialize)]
#[repr(transparent)]
pub struct Reaction(pub HashMap<CompactString, u64>);

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionAOE {
    pub count: Reaction,
    pub current_user_reactions: Vec<CompactString>,
}

pub async fn get_area_of_effect<I>(eids: I, uid: Option<&str>, db: &mut Client) -> DBResult<HashMap<i32, ReactionAOE>>
where
    I: Iterator<Item = i32> + Clone + fmt::Debug + Sync,
{
    const SQL_U: &str = "select eid, emoji, count(*), bool_or($1 = uid) from lean4oj.discussion_reactions where eid = any($2) group by eid, emoji;";
    const SQL_N: &str = "select eid, emoji, count(*) from lean4oj.discussion_reactions where eid = any($1) group by eid, emoji;";

    let mut lookup = HashMap::with_capacity(eids.clone().size_hint().0);

    if let Some(uid) = uid {
        let stmt = db.prepare_static(SQL_U.into()).await?;
        let params: [&(dyn ToSql + Sync); 2] = [&uid, &ToSqlIterUnsafe(eids)];
        let stream = db.query_raw(&stmt, params).await?;
        stream.try_for_each(|row| ready(try {
            let eid = row.try_get::<_, i32>(0)?;
            let emoji = CompactString::new(row.try_get::<_, &str>(1)?);
            let count = row.try_get::<_, i64>(2)?.cast_unsigned();
            let curr = row.try_get::<_, bool>(3)?;
            let aoe: &mut ReactionAOE = lookup.entry(eid).or_default();
            if curr {
                aoe.current_user_reactions.push(emoji.clone());
            }
            aoe.count.0.insert(emoji, count);
        })).await?;
    } else {
        let stmt = db.prepare_static(SQL_N.into()).await?;
        let stream = db.query_raw(&stmt, [ToSqlIterUnsafe(eids)]).await?;
        stream.try_for_each(|row| ready(try {
            let eid = row.try_get::<_, i32>(0)?;
            let emoji = CompactString::new(row.try_get::<_, &str>(1)?);
            let count = row.try_get::<_, i64>(2)?.cast_unsigned();
            let aoe = lookup.entry(eid).or_default();
            aoe.count.0.insert(emoji, count);
        })).await?;
    }

    Ok(lookup)
}
