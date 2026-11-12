use core::{
    fmt::{self, Write},
    future::ready,
};
use std::{
    collections::btree_map::{BTreeMap, Keys},
    time::SystemTime,
};

use bytes::Bytes;
use compact_str::CompactString;
use futures_util::TryStreamExt;
use serde::{
    Deserialize, Serialize,
    ser::{SerializeMap, SerializeSeq},
};
use smallvec::{SmallVec, smallvec};
use tokio_postgres::{
    Client, Row,
    types::{Json, ToSql},
};

use crate::{
    libs::{
        db::{DBResult, JsonChecked, ToSqlIter},
        util::get_millis,
    },
    models::localedict::LocaleDict,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemContentSection {
    pub section_title: CompactString,
    pub text: CompactString,
    // no sample needed!
}

impl Serialize for ProblemContentSection {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("sectionTitle", &self.section_title)?;
        map.serialize_entry("type", "Text")?;
        map.serialize_entry("text", &self.text)?;
        map.end()
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemInner {
    pub title: CompactString,
    pub content_sections: Vec<ProblemContentSection>,
}

pub struct Problem {
    pub pid: i32, // negative for draft
    pub is_public: bool,
    pub public_at: SystemTime,
    pub owner: CompactString,
    pub content: LocaleDict<ProblemInner>,
    pub sub: u32,
    pub ac: u32,
    pub submittable: bool,
    pub jb: Bytes,
}

#[repr(transparent)]
struct Inner1<'a, T>(pub Keys<'a, CompactString, T>);

impl Serialize for Inner1<'_, ProblemInner> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(self.0.size_hint().1)?;
        for locale in self.0.clone() {
            seq.serialize_element(&**locale)?;
        }
        seq.end()
    }
}

impl Serialize for Problem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.pid)?;
        map.serialize_entry("displayId", &self.pid)?;
        map.serialize_entry("type", "Lean")?;
        map.serialize_entry("isPublic", &self.is_public)?;
        map.serialize_entry("publicTime", &get_millis(self.public_at))?;
        map.serialize_entry("ownerId", &self.owner)?;
        map.serialize_entry("locales", &Inner1(self.content.0.keys()))?;
        map.serialize_entry("submissionCount", &self.sub)?;
        map.serialize_entry("acceptedSubmissionCount", &self.ac)?;
        map.end()
    }
}

impl TryFrom<Row> for Problem {
    type Error = tokio_postgres::Error;

    fn try_from(row: Row) -> Result<Self, Self::Error> {
        let pid = row.try_get("pid")?;
        let is_public = row.try_get("is_public")?;
        let public_at = row.try_get("public_at")?;
        let owner = row.try_get::<_, &str>("owner")?.into();
        let Json(content) = row.try_get("pcontent")?;
        let sub = row.try_get::<_, i32>("sub")?.cast_unsigned();
        let ac = row.try_get::<_, i32>("pac")?.cast_unsigned();
        let submittable = row.try_get("submittable")?;
        let jb = row.try_get::<_, JsonChecked>("jb")?;
        let jb = row.buffer_bytes().slice_ref(jb.0);
        // keep it unparsed if not needed.

        Ok(Self { pid, is_public, public_at, owner, content, sub, ac, submittable, jb })
    }
}

#[inline]
fn 𝒯(row: Row) -> DBResult<(Problem, Vec<u32>)> {
    let problem = row.clone().try_into()?;
    let tag_ids = row.try_get::<_, Vec<Option<i32>>>("tags")?;
    Ok((problem, tag_ids.into_iter().filter_map(|x| x.map(i32::cast_unsigned)).collect()))
}

impl Problem {
    pub async fn by_pid(pid: i32, db: &mut Client) -> DBResult<Option<Self>> {
        const SQL: &str = "select pid, is_public, public_at, owner, pcontent, sub, pac, submittable, jb from lean4oj.problems where pid = $1";

        let stmt = db.prepare_static(SQL.into()).await?;
        let result = match db.query_opt(&stmt, &[&pid]).await? {
            Some(row) => Some(row.try_into()?),
            None => None,
        };
        Ok(result)
    }

    pub async fn by_pid_uid(pid: i32, uid: &str, db: &mut Client) -> DBResult<Option<Self>> {
        const SQL: &str = "select pid, is_public, public_at, owner, pcontent, sub, pac, submittable, jb from lean4oj.problems where pid = $1 and (owner = $2 or is_public)";

        let stmt = db.prepare_static(SQL.into()).await?;
        let result = match db.query_opt(&stmt, &[&pid, &uid]).await? {
            Some(row) => Some(row.try_into()?),
            None => None,
        };
        Ok(result)
    }

    pub async fn create(owner: &str, content: &LocaleDict<ProblemInner>, db: &mut Client) -> DBResult<i32> {
        const SQL: &str = "insert into lean4oj.problems (owner, pcontent) values ($1, $2) returning pid";

        let stmt = db.prepare_static(SQL.into()).await?;
        let content: *const Json<BTreeMap<CompactString, ProblemInner>> = (&raw const content.0).cast();
        let row = db.query_one(&stmt, &[&owner, unsafe { &*content }]).await?;
        row.try_get(0)
    }

    pub async fn set_tags<I>(pid: i32, tags: I, db: &mut Client) -> DBResult<()>
    where
        I: ExactSizeIterator<Item = u32> + Clone + fmt::Debug + Sync,
    {
        const SQL_INSERT: &str = "insert into lean4oj.problem_tags (pid, tid) select $1, unnest($2::integer[]) on conflict (pid, tid) do nothing";
        const SQL_DELETE: &str = "delete from lean4oj.problem_tags where pid = $1 and tid != all ($2::integer[])";

        let stmt_insert = db.prepare_static(SQL_INSERT.into()).await?;
        let stmt_delete = db.prepare_static(SQL_DELETE.into()).await?;
        let txn = db.transaction().await?;
        txn.execute(&stmt_insert, &[&pid, &ToSqlIter(tags.clone().map(u32::cast_signed))]).await?;
        txn.execute(&stmt_delete, &[&pid, &ToSqlIter(tags.map(u32::cast_signed))]).await?;
        txn.commit().await
    }

    pub async fn search_aoe<'a, F>(skip: i64, take: i64, tag_ids: Option<&'a [i32]>, extend: F, db: &mut Client) -> DBResult<Vec<(Self, Vec<u32>)>>
    where
        F: FnOnce(String, SmallVec<[&'a (dyn ToSql + Sync); 8]>) -> (String, SmallVec<[&'a (dyn ToSql + Sync); 8]>),
    {
        let mut sql = "(select pid, is_public, public_at, owner, pcontent, sub, pac, submittable, jb, array_agg(tid) as tags from lean4oj.problems natural left join lean4oj.problem_tags where pid >= 0".to_owned();
        let pos1 = sql.len();
        let mut args: SmallVec<[&(dyn ToSql + Sync); 8]> = smallvec![
            unsafe { core::mem::transmute::<&i64, &'a i64>(&skip) } as _,
            unsafe { core::mem::transmute::<&i64, &'a i64>(&take) } as _,
        ];
        (sql, args) = extend(sql, args);
        let pos2 = sql.len();
        sql.push_str(" group by pid");
        if let Some(ref tag_ids) = tag_ids {
            let _ = write!(&mut sql, " having ${} <@ array_agg(tid)", args.len() + 1);
            args.push(unsafe { core::mem::transmute::<&&'a [i32], &'a &'a [i32]>(tag_ids) });
        }
        sql.push_str(" order by pid) union all (select pid, is_public, public_at, owner, pcontent, sub, pac, submittable, jb, array_agg(tid) as tags from lean4oj.problems natural left join lean4oj.problem_tags where pid < 0");
        sql.extend_from_within(pos1..pos2);
        sql.push_str(" group by pid");
        if tag_ids.is_some() {
            let _ = write!(&mut sql, " having ${} <@ array_agg(tid)", args.len());
        }
        sql.push_str(" order by pid desc) offset $1 limit $2");

        let stmt = db.prepare_static(sql.into()).await?;
        let stream = db.query_raw(&stmt, args).await?;
        stream.and_then(|row| ready(𝒯(row))).try_collect().await
    }

    pub async fn count_aoe<'a, F>(tag_ids: Option<&'a [i32]>, extend: F, db: &mut Client) -> DBResult<u64>
    where
        F: FnOnce(String, SmallVec<[&'a (dyn ToSql + Sync); 8]>) -> (String, SmallVec<[&'a (dyn ToSql + Sync); 8]>),
    {
        let mut sql = "select count(*) from (select from lean4oj.problems natural left join lean4oj.problem_tags where true".to_owned();
        let mut args: SmallVec<[&(dyn ToSql + Sync); 8]> = smallvec![];
        (sql, args) = extend(sql, args);
        sql.push_str(" group by pid");
        if let Some(ref tag_ids) = tag_ids {
            let _ = write!(&mut sql, " having ${} <@ array_agg(tid)", args.len() + 1);
            args.push(unsafe { core::mem::transmute::<&&'a [i32], &'a &'a [i32]>(tag_ids) });
        }
        sql.push(')');

        let stmt = db.prepare_static(sql.into()).await?;
        let row = db.query_one(&stmt, &args).await?;
        row.try_get::<_, i64>(0).map(i64::cast_unsigned)
    }
}
