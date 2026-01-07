use core::pin::pin;

use compact_str::CompactString;
use futures_util::TryStreamExt;
use smallvec::SmallVec;
use tokio_postgres::Client;

use super::db::{DBError, DBResult};

pub const COUNT: usize = 9;
pub type Privileges = SmallVec<[CompactString; COUNT]>;

pub async fn check(uid: &str, privi: &str, db: &mut Client) -> DBResult<bool> {
    const SQL: &str = "select 1 from lean4oj.user_groups where uid = $1 and (gid = $2 or gid = 'Lean4OJ.Admin') limit 1";

    let stmt = db.prepare_static(SQL.into()).await?;
    db.query_opt(&stmt, &[&uid, &privi]).await.map(|x| x.is_some())
}

pub async fn all(uid: &str, db: &mut Client) -> DBResult<Privileges> {
    const SQL: &str = "select gid from lean4oj.user_groups where uid = $1 and gid like 'Lean4OJ.%'";

    let stmt = db.prepare_static(SQL.into()).await?;
    let stream = db.query_raw(&stmt, [uid]).await?;
    let mut stream = pin!(stream);
    let mut ret = SmallVec::new();
    while let Some(row) = stream.try_next().await? {
        let group = row.try_get::<_, &str>(0)?;
        let Some(perm) = group.strip_prefix("Lean4OJ.") else {
            return Err(DBError::new(
                tokio_postgres::error::Kind::FromSql(1), Some("invalid SQL response".into()),
            ));
        };
        ret.push(perm.into());
    }

    Ok(ret)
}

pub fn is_admin(privi: &Privileges) -> bool {
    privi.iter().any(|p| p == "Admin")
}
