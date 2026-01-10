use core::{fmt, future::ready};

use compact_str::CompactString;
use futures_util::TryStreamExt;
use hashbrown::HashMap;
use smallvec::SmallVec;
use tokio_postgres::{Client, Row};

use crate::{
    libs::db::ToSqlIter,
    models::user::{User, UserAOE},
};

use super::db::{DBError, DBResult};

/// `Admin`, `EditHomepage`, `Judger`, `ManageContest`, `ManageDiscussion`, `ManageProblem`, `ManageUser`, `ManageUserGroup`, `TooManyOLeans`
pub const COUNT: usize = 9;
pub type Privileges = SmallVec<[CompactString; COUNT]>;

pub async fn check(uid: &str, privi: &str, db: &mut Client) -> DBResult<bool> {
    const SQL: &str = "select from lean4oj.user_groups where uid = $1 and (gid = $2 or gid = 'Lean4OJ.Admin') limit 1";

    let stmt = db.prepare_static(SQL.into()).await?;
    db.query_opt(&stmt, &[&uid, &privi]).await.map(|x| x.is_some())
}

fn 𝒻(row: Row) -> DBResult<CompactString> {
    let group = row.try_get::<_, &str>(0)?;
    if let Some(perm) = group.strip_prefix("Lean4OJ.") {
        Ok(perm.into())
    } else {
        Err(DBError::new(tokio_postgres::error::Kind::FromSql(1), Some("invalid SQL response".into())))
    }
}

pub async fn all(uid: &str, db: &mut Client) -> DBResult<Privileges> {
    const SQL: &str = "select gid from lean4oj.user_groups where uid = $1 and gid like 'Lean4OJ.%'";

    let stmt = db.prepare_static(SQL.into()).await?;
    let stream = db.query_raw(&stmt, [uid]).await?;
    stream.and_then(|row| ready(𝒻(row))).try_collect().await
}

#[inline]
pub fn is_admin(privi: &Privileges) -> bool {
    privi.iter().any(|p| p == "Admin")
}

pub async fn get_area_of_effect<'a, I>(uids: I, db: &mut Client) -> DBResult<HashMap<CompactString, UserAOE>>
where
    I: ExactSizeIterator<Item = &'a str> + Clone + fmt::Debug,
{
    const SQL_USER: &str = "select uid, password, username, email, register_time, ac, nickname, bio, avatar_info from lean4oj.users where uid = any($1)";
    const SQL_PRIVILEGE: &str = "select uid, gid from lean4oj.user_groups where uid = any($1) and gid like 'Lean4OJ.%'";

    let stmt_user = db.prepare_static(SQL_USER.into()).await?;
    let stmt_priv = db.prepare_static(SQL_PRIVILEGE.into()).await?;

    let mut lookup = HashMap::with_capacity(uids.len());

    let stream_user = db.query_raw(&stmt_user, [ToSqlIter(uids.clone())]).await?;
    let stream_priv = db.query_raw(&stmt_priv, [ToSqlIter(uids)]).await?;

    stream_user.try_for_each(|row| ready(try {
        let user = User::try_from(row)?;
        lookup.insert(user.uid.clone(), UserAOE {
            user,
            is_admin: false,
            is_problem_admin: false,
            is_contest_admin: false,
            is_discussion_admin: false,
        });
    })).await?;

    stream_priv.try_for_each(|row| ready(try {
        let uid = row.try_get::<_, &str>(0)?;
        let group = row.try_get::<_, &str>(1)?;
        if let Some(entry) = lookup.get_mut(uid) {
            match group {
                "Lean4OJ.Admin" => entry.is_admin = true,
                "Lean4OJ.ManageProblem" => entry.is_problem_admin = true,
                "Lean4OJ.ManageContest" => entry.is_contest_admin = true,
                "Lean4OJ.ManageDiscussion" => entry.is_discussion_admin = true,
                _ => {}
            }
        }
    })).await?;

    Ok(lookup)
}
